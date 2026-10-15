//! Shared main-window drag for empty stretches of the UI (topbar, sidebar,
//! live feed).
//!
//! The "is this an empty area?" hit-test and the gesture state machine run
//! in JS ([`SCRIPT`], because Dioxus mouse events don't expose the DOM
//! target); presses, moves (past a 5px threshold, so plain clicks never
//! nudge the window), and releases stream in over the eval channel. The
//! first move hands the gesture to the OS-native window drag — the same
//! `drag()` the PiP window uses — which tracks the cursor at display rate,
//! across monitors, and outside the window, none of which manual
//! `set_outer_position` can do reliably. If the OS doesn't take over (move
//! events keep arriving, e.g. on Wayland), the next move falls through to
//! the manual path: `set_outer_position` coalesced to ~15 fps, with the
//! trailing position flushed on release so the window never ends a drag one
//! frame behind the cursor.

use dioxus::prelude::spawn;
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

/// Minimum interval between `set_outer_position` calls in the manual
/// fallback (~15 fps).
const MOVE_INTERVAL: Duration = Duration::from_millis(66);

struct DragState {
    /// Screen point where the press landed.
    anchor: (f64, f64),
    /// Window position at press time.
    origin: (f64, f64),
    /// True once the first move message arrives (JS gates on the threshold).
    moving: bool,
    /// The native handoff is attempted once per drag, on the first move.
    native_attempted: bool,
    last_applied: Instant,
    /// Latest coalesced target not yet applied (manual fallback).
    pending: Option<(f64, f64)>,
    /// Bumped on every `begin` so a flush task scheduled by a previous
    /// drag can't move the window mid-next-drag.
    generation: u64,
}

thread_local! {
    static STATE: RefCell<Option<DragState>> = const { RefCell::new(None) };
    /// Generation of the drag a flush task is already scheduled for
    /// (`u64::MAX` = none), so `track` doesn't spawn one per mousemove.
    static FLUSH_SCHEDULED: Cell<u64> = const { Cell::new(u64::MAX) };
}

/// Installed once at the app root. A primary mousedown on an empty stretch
/// of a `[data-window-drag]` region starts a window drag; interactive
/// elements (and `[data-no-window-drag]` subtrees, e.g. device cards) are
/// excluded so their clicks and the sidebar's own device drag keep working.
/// Pointer capture keeps pointermove/up arriving when the cursor leaves
/// the window mid-drag (the webview otherwise stops at its edge). The
/// `window-dragging` class forces the grabbing cursor + no text selection
/// for the duration of the press (see `.dragging-active`).
///
/// Messages to [`on_message`]: `[0, x, y]` press, `[1, x, y]` move (only
/// once the 5px threshold passes), `[2, 0, 0]` release/cancel.
///
/// The contextmenu listener suppresses the webview's dev menu (Reload /
/// Inspect Element) app-wide — the app handles its own right-clicks —
/// except on text fields, where the native Cut/Copy/Paste menu is useful.
pub const SCRIPT: &str = r#"
(() => {
    let drag = null; // {x, y, sent}: press point + whether the threshold passed
    document.addEventListener('mousedown', (e) => {
        document.documentElement.classList.remove('window-dragging');
        if (e.button !== 0) return;
        const t = e.target instanceof Element ? e.target : e.target.parentElement;
        if (!t || !t.closest('[data-window-drag]')) return;
        if (t.closest('button, a, input, select, textarea, option, [contenteditable], [data-no-window-drag]')) return;
        e.preventDefault();
        try { t.setPointerCapture(e.pointerId); } catch (_) {}
        document.documentElement.classList.add('window-dragging');
        drag = { x: e.screenX, y: e.screenY, sent: false };
        dioxus.send([0, e.screenX, e.screenY]);
    });
    window.addEventListener('pointermove', (e) => {
        if (!drag) return;
        if (!drag.sent) {
            const dx = e.screenX - drag.x, dy = e.screenY - drag.y;
            if (dx * dx + dy * dy < 25) return;
            drag.sent = true;
        }
        dioxus.send([1, e.screenX, e.screenY]);
    });
    const finish = () => {
        if (!drag) return;
        drag = null;
        document.documentElement.classList.remove('window-dragging');
        dioxus.send([2, 0, 0]);
    };
    window.addEventListener('pointerup', finish);
    window.addEventListener('pointercancel', finish);
})();
document.addEventListener('contextmenu', (e) => {
    const t = e.target instanceof Element ? e.target : null;
    if (t && t.closest('input, textarea, [contenteditable]')) return;
    e.preventDefault();
});
"#;

/// Dispatch one gesture message from [`SCRIPT`].
pub fn on_message(kind: u8, x: f64, y: f64) {
    match kind {
        0 => begin(x, y),
        1 => track(x, y),
        _ => end(),
    }
}

/// Begin a drag with the pointer at `(x, y)` in screen coordinates.
/// Ignored in fullscreen, where window positions are meaningless.
pub fn begin(x: f64, y: f64) {
    let window = dioxus::desktop::window();
    if window.window.fullscreen().is_some() {
        return;
    }
    let Ok(pos) = window.window.outer_position() else {
        return;
    };
    let pos = pos.to_logical::<f64>(window.window.scale_factor());
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let generation = s.as_ref().map_or(0, |d| d.generation.wrapping_add(1));
        *s = Some(DragState {
            anchor: (x, y),
            origin: (pos.x, pos.y),
            moving: false,
            native_attempted: false,
            // Backdated so the first qualifying move applies immediately
            // instead of up to MOVE_INTERVAL late.
            last_applied: Instant::now()
                .checked_sub(MOVE_INTERVAL)
                .unwrap_or_else(Instant::now),
            pending: None,
            generation,
        });
    });
}

/// Handle a drag move at `(x, y)` in screen coordinates (JS has already
/// applied the press threshold). No-op without an active drag. The first
/// move hands the gesture to the OS-native drag; if that doesn't take over
/// the pointer keeps reporting, and later moves use the manual path —
/// coalesced to `MOVE_INTERVAL`, trailing position flushed by the
/// scheduled task or by [`end`].
pub fn track(x: f64, y: f64) {
    enum Action {
        Apply(f64, f64),
        ScheduleFlush(u64),
        TryNative,
    }
    let action = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let d = s.as_mut()?;
        if !d.moving {
            d.moving = true;
            if !d.native_attempted {
                d.native_attempted = true;
                return Some(Action::TryNative);
            }
        }
        // On macOS the native handoff reliably takes over (the PiP window
        // relies on the same `drag()` call). Commit to it: move events
        // that still trickle in during an OS drag must not also drive the
        // manual path — double-applying reads as shaking. On other
        // platforms arriving moves mean the handoff failed, so they fall
        // through to the manual fallback below.
        #[cfg(target_os = "macos")]
        if d.native_attempted {
            return None;
        }
        let target = (d.origin.0 + x - d.anchor.0, d.origin.1 + y - d.anchor.1);
        if d.last_applied.elapsed() >= MOVE_INTERVAL {
            d.last_applied = Instant::now();
            d.pending = None;
            Some(Action::Apply(target.0, target.1))
        } else {
            d.pending = Some(target);
            let generation = d.generation;
            let fresh = FLUSH_SCHEDULED.with(|f| f.replace(generation)) != generation;
            fresh.then_some(Action::ScheduleFlush(generation))
        }
    });
    match action {
        // If the OS takes over here it consumes the rest of the gesture:
        // no more move/release messages arrive and the state sits idle
        // until the next press resets it. If it doesn't (Wayland), the
        // next move message falls through to the manual path.
        Some(Action::TryNative) => dioxus::desktop::window().drag(),
        Some(Action::Apply(x, y)) => apply(x, y),
        Some(Action::ScheduleFlush(generation)) => {
            spawn(async move {
                tokio::time::sleep(MOVE_INTERVAL).await;
                flush(generation);
            });
        }
        None => {}
    }
}

/// End the drag, applying the last coalesced position so the window
/// doesn't lag one frame behind the release.
pub fn end() {
    let target = STATE.with(|s| s.borrow_mut().take().and_then(|d| d.pending));
    if let Some((x, y)) = target {
        apply(x, y);
    }
}

/// Apply the pending position if this flush still belongs to the active
/// drag (a stale task from an earlier or finished drag does nothing).
fn flush(generation: u64) {
    let target = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let d = s.as_mut()?;
        if d.generation != generation {
            return None;
        }
        d.last_applied = Instant::now();
        d.pending.take()
    });
    if let Some((x, y)) = target {
        apply(x, y);
    }
}

fn apply(x: f64, y: f64) {
    dioxus::desktop::window()
        .window
        .set_outer_position(dioxus::desktop::LogicalPosition::new(x, y));
}
