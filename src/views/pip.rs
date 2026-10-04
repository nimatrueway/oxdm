#![allow(non_snake_case)]
//! Picture-in-picture: a second always-on-top window showing one stream.
//!
//! Separate `VirtualDom` and WebView, so nothing from the main window's
//! context is available here — the CSS and player script are injected
//! again and the source URL comes in as a prop. Closing the window drops
//! the player; the RTSP session behind it idles out on its own.
//!
//! Frameless: a mousedown anywhere on the video starts a native window
//! drag (the player's control bar swallows its own mousedowns), and a
//! hover-only × in the corner closes it. On macOS the title bar and traffic
//! lights are hidden, but the window stays titled and resizable so the
//! system still resizes it from the edges. `decorations(false)` and
//! `titlebar_hidden` force a borderless mask, which has no resize edges.
//!
//! Once the player reports the source dimensions the window snaps to that
//! aspect ratio and (on macOS) locks it, so resizing never shows letterbox
//! bars around the video.

use crate::components::Icon;
use crate::state::Locale;
use crate::video::VideoSource;
use crate::views::live_video::VideoPlayer;
use dioxus::prelude::*;

const PIP_WIDTH: f64 = 480.0;
const PIP_HEIGHT: f64 = 270.0;
const PIP_MIN_W: f64 = 200.0;
const PIP_MIN_H: f64 = 120.0;
/// Edge band (CSS px) left to the OS resize handles instead of window drag.
const EDGE_PX: f64 = 8.0;

// `oxdm-videosize` comes from the <oxdm-stream> player; the capturing `load`
// listener covers the plain <img> embed, whose events don't bubble.
const SIZE_SCRIPT: &str = r#"
    const send = (w, h) => { if (w > 0 && h > 0) dioxus.send([w, h]); };
    document.addEventListener('oxdm-videosize', e => send(e.detail.width, e.detail.height));
    document.addEventListener('load', e => {
        if (e.target && e.target.tagName === 'IMG') send(e.target.naturalWidth, e.target.naturalHeight);
    }, true);
"#;

/// Fit the window to `width:height`, keeping the current width.
fn lock_aspect(width: u32, height: u32) {
    let window = dioxus::desktop::window();
    let win = &window.window;
    let (w, h) = (f64::from(width), f64::from(height));
    #[cfg(target_os = "macos")]
    {
        use dioxus::desktop::tao::platform::macos::WindowExtMacOS;
        use objc::runtime::{Object, Sel};
        use objc::Message;
        #[repr(C)]
        struct NsSize {
            width: f64,
            height: f64,
        }
        let ns_window = win.ns_window() as *mut Object;
        // AppKit constrains the frame to integral multiples of this size, so
        // the raw 2304x1296 would step in 16px jumps (or worse). w/h : 1 is
        // what Electron passes; it keeps every pixel width reachable.
        let size = NsSize {
            width: w / h,
            height: 1.0,
        };
        // `Sel::register` instead of `sel!`: objc 0.2's macro trips
        // `unexpected_cfgs` under clippy -D warnings.
        let sel = Sel::register("setContentAspectRatio:");
        unsafe {
            if let Some(obj) = ns_window.as_ref() {
                let _: Result<(), _> = obj.send_message(sel, (size,));
            }
        }
    }
    // Keep the min size on-ratio; a mismatched floor makes AppKit fight itself.
    win.set_min_inner_size(Some(dioxus::desktop::LogicalSize::new(
        PIP_MIN_W,
        PIP_MIN_W * h / w,
    )));
    let cur_w = f64::from(win.inner_size().width) / win.scale_factor();
    win.set_inner_size(dioxus::desktop::LogicalSize::new(cur_w, cur_w * h / w));
}

/// Open `source` in a new floating window titled `title`.
pub fn open(source: VideoSource, title: String, locale: Locale) {
    let main_window = std::rc::Rc::downgrade(&dioxus::desktop::window());
    let props = PipWindowProps {
        source: source.clone(),
        locale,
        return_to_main: EventHandler::new(move |_| {
            if let Some(main_window) = main_window.upgrade() {
                main_window.window.set_minimized(false);
                main_window.window.set_visible(true);
                main_window.window.set_focus();
            }
        }),
    };
    let dom = VirtualDom::new_with_props(PipWindow, props);
    let cfg = dioxus::desktop::Config::new()
        .with_menu(None)
        .with_background_color((0, 0, 0, 255))
        .with_window(frameless_builder(title));
    dioxus::desktop::window().new_window(dom, cfg);
    tracing::info!(id = %source.id, "PiP window opened");
}

fn frameless_builder(title: String) -> dioxus::desktop::WindowBuilder {
    let builder = dioxus::desktop::WindowBuilder::new()
        .with_title(format!("{title} — OxDM"))
        .with_window_icon(crate::load_window_icon())
        .with_always_on_top(true)
        .with_resizable(true)
        .with_inner_size(dioxus::desktop::LogicalSize::new(PIP_WIDTH, PIP_HEIGHT))
        .with_min_inner_size(dioxus::desktop::LogicalSize::new(PIP_MIN_W, PIP_MIN_H));
    hide_chrome(builder)
}

/// Hide the title bar without dropping the resizable style mask.
#[cfg(target_os = "macos")]
fn hide_chrome(builder: dioxus::desktop::WindowBuilder) -> dioxus::desktop::WindowBuilder {
    use dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS;
    builder
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_titlebar_buttons_hidden(true)
        .with_fullsize_content_view(true)
}

#[cfg(not(target_os = "macos"))]
fn hide_chrome(builder: dioxus::desktop::WindowBuilder) -> dioxus::desktop::WindowBuilder {
    builder.with_decorations(false)
}

#[component]
fn PipWindow(source: VideoSource, locale: Locale, return_to_main: EventHandler) -> Element {
    use_future(|| async {
        let mut sizes = document::eval(SIZE_SCRIPT);
        let mut last = None;
        while let Ok((w, h)) = sizes.recv::<(u32, u32)>().await {
            if last != Some((w, h)) {
                last = Some((w, h));
                lock_aspect(w, h);
            }
        }
    });

    rsx! {
        document::Style { {crate::MAIN_CSS} }
        document::Script { {crate::STREAM_JS} }
        div {
            class: "pip-root",
            onmousedown: move |e| {
                if e.data().trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                    return;
                }
                let c = e.data().client_coordinates();
                let window = dioxus::desktop::window();
                let scale = window.window.scale_factor();
                let size = window.webview.bounds()
                    .map(|bounds| bounds.size.to_logical::<f64>(scale))
                    .unwrap_or_else(|_| window.window.inner_size().to_logical::<f64>(scale));
                let near_edge = c.x < EDGE_PX || c.y < EDGE_PX || c.x > size.width - EDGE_PX || c.y > size.height - EDGE_PX;
                if !near_edge {
                    window.drag();
                }
            },
            VideoPlayer { source, locale }
            button {
                class: "pip-close",
                title: "Close",
                onmousedown: move |e| e.stop_propagation(),
                onclick: move |_| {
                    dioxus::desktop::window().close();
                    return_to_main.call(());
                },
                Icon { name: "x", size: 14 }
            }
        }
    }
}
