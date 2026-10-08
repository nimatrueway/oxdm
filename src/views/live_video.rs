#![allow(non_snake_case)]
use crate::components::{ContextMenu, DevicePanel, Icon, ProfileSelector};
use crate::i18n;
use crate::state::{Credentials, Ctx, View};
use crate::video::{self, EmbedKind};
use crate::views::{
    imaging::ImagingView,
    ptz::{DragPanButton, PanPositionOverlay, PanPreview, PtzControlView},
};
use dioxus::prelude::*;

/// Which video backend a stage should use for the current view.
/// Defaults to Rtsp — real frame rate + audio through the in-process
/// pipeline. Snapshot (MJPEG polling) stays one tab away for cameras
/// whose RTSP is broken or blocked. Preference is per-session
/// (intentionally not persisted yet — most users will pick once and
/// stay).
///
/// Shared by the live player and its inline camera controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LiveVideoMode {
    Snapshot,
    #[default]
    Rtsp,
}

impl LiveVideoMode {
    pub fn backend_id(self) -> &'static str {
        match self {
            Self::Snapshot => "mjpeg",
            Self::Rtsp => "rtsp",
        }
    }
}

/// Snapshot/RTSP choice inside the live player's playback options.
#[component]
pub fn LiveModeTabs(mode: Signal<LiveVideoMode>) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    rsx! {
        div { class: "live-video-modes",
            ModeTab {
                active: *mode.read() == LiveVideoMode::Snapshot,
                label: i18n::t(locale, "live_mode_snapshot"),
                title: i18n::t(locale, "live_mode_snapshot_hint"),
                onclick: {
                    let mut mode = mode;
                    move |_| mode.set(LiveVideoMode::Snapshot)
                },
            }
            ModeTab {
                active: *mode.read() == LiveVideoMode::Rtsp,
                label: i18n::t(locale, "live_mode_rtsp"),
                title: i18n::t(locale, "live_mode_rtsp_hint"),
                onclick: {
                    let mut mode = mode;
                    move |_| mode.set(LiveVideoMode::Rtsp)
                },
            }
        }
    }
}

/// Live player with profile selection and optional camera controls.
#[component]
pub fn LiveVideoView(
    addr: ReadSignal<String>,
    creds: Memo<Credentials>,
    gate: crate::api::DeviceGate,
) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let mode = use_signal(LiveVideoMode::default);
    // Theater lives in Ctx: this component is keyed by device address, so a
    // camera switch from the theater right-click menu remounts it and a
    // local signal would exit theater mode behind the user's back.
    let mut theater = ctx.theater;
    let mut pre_fullscreen_theater = use_signal(|| false);
    let mut drag_anchor = use_signal(|| None::<(f64, f64, f64, f64)>);
    let mut menu_open = use_signal(|| false);
    // Position of the theater right-click camera switcher.
    let mut switch_menu = use_signal(|| None::<(f64, f64)>);
    let window = use_hook(dioxus::desktop::window);
    let drag_window = window.clone();
    let decorations = use_hook(|| window.window.is_decorated());
    let theater_window = window.clone();
    use_effect(move || {
        let active = *theater.read();
        theater_window
            .window
            .set_decorations(decorations && !active);
        crate::set_main_window_theater(&theater_window.window, active);
    });
    let drop_window = window.clone();
    use_drop(move || {
        // Theater is global and this component is keyed by device address:
        // switching cameras from the theater right-click menu unmounts this
        // instance and mounts a fresh one whose effect re-applies theater.
        // Tearing the window state down here would flicker the titlebar and
        // drop fullscreen for nothing. With theater already off, the effect
        // above has restored the window, so this is just a safety net.
        if *theater.peek() {
            return;
        }
        drop_window.window.set_decorations(decorations);
        crate::set_main_window_theater(&drop_window.window, false);
    });
    let ptz_speed = ctx.ptz_speed;
    let pan_preview = use_signal(PanPreview::default);
    let profile_sig = ctx.selected_profile;
    let active_view = *ctx.view.read();
    let controls_open = matches!(active_view, View::PtzControl | View::ImagingSettings);
    let profile_key = profile_sig.read().clone().unwrap_or_default();

    // Memo so LiveVideoStage's use_resource sees the backend choice
    // as a reactive dep — Dioxus only re-runs a resource when signals
    // it reads change, so passing a plain value doesn't trigger a
    // re-fetch on tab switch.
    let backend_id = use_memo(move || mode.read().backend_id());
    let backend_display = match *mode.read() {
        LiveVideoMode::Snapshot => video::mjpeg(),
        LiveVideoMode::Rtsp => video::rtsp(),
    }
    .map(|b| b.display_name());

    // Saving the live frame directly isn't possible — the stream lives in
    // the webview (an <img>/<video> URL), not in Rust memory. So we grab a
    // fresh JPEG via GetSnapshotUri on click, which works in both Snapshot
    // and RTSP modes; cameras without a usable snapshot URI (Tapo) get a
    // key frame decoded out of the RTSP stream instead. Disabled until a
    // device + profile are selected.
    let can_save = !addr.read().is_empty() && profile_sig.read().is_some();

    // Recording is keyed by the RTSP stream id; poll its status once a
    // second so the elapsed counter ticks and a recording started before
    // navigating away is still shown when the view is re-entered.
    let stream_id = use_memo(move || {
        let token = profile_sig.read().clone().unwrap_or_default();
        video::rtsp::stream_id_for(&addr.read(), &token)
    });
    let mut rec_elapsed = use_signal(|| None::<u64>);
    use_future(move || async move {
        loop {
            let secs =
                video::rtsp::recording(&stream_id.read()).map(|s| s.started.elapsed().as_secs());
            if secs != *rec_elapsed.peek() {
                rec_elapsed.set(secs);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
    let is_recording = rec_elapsed.read().is_some();
    let can_record = can_save && *mode.read() == LiveVideoMode::Rtsp;
    let rec_icon = if is_recording {
        "stop-circle"
    } else {
        "record"
    };
    let theater_open = *theater.read();
    let switch_pos = *switch_menu.read();
    // Read guards for the theater switcher menu; held across the render the
    // same way DeviceList holds its device list.
    let devices = ctx.devices.read();
    let sel = *ctx.selected.read();

    rsx! {
        div {
            class: if theater_open { "live-video-view live-video-view--theater" } else { "live-video-view" },
            tabindex: "0",
            onkeydown: move |event| {
                if event.key() != dioxus::html::input_data::keyboard_types::Key::Escape {
                    return;
                }
                // Escape closes the switcher first; only when it is not
                // open does it leave theater mode.
                if switch_menu.peek().is_some() {
                    event.stop_propagation();
                    switch_menu.set(None);
                } else if theater() {
                    event.stop_propagation();
                    dioxus::desktop::window().set_fullscreen(false);
                    theater.set(*pre_fullscreen_theater.peek());
                    // Consume the saved state so a later Escape (plain
                    // theater, no fullscreen) exits instead of restoring.
                    pre_fullscreen_theater.set(false);
                }
            },
            oncontextmenu: move |e: Event<MouseData>| {
                // Theater hides the sidebar, so the whole theater surface
                // carries the camera switcher. Outside theater the native
                // menu is left alone.
                if !theater() {
                    return;
                }
                e.prevent_default();
                let c = e.data().client_coordinates();
                switch_menu.set(Some((c.x, c.y)));
            },
            div { class: "content-header live-toolbar",
                ProfileSelector { addr, creds }
                for (target, available, icon, label) in [
                    (View::PtzControl, gate.ptz, "crosshair", "workspace_ptz"),
                    (View::ImagingSettings, gate.imaging, "sliders", "workspace_adjustments"),
                ] {
                    if available || active_view == target {
                        button {
                            class: if active_view == target { "icon-btn live-control-toggle live-control-toggle--active" } else { "icon-btn live-control-toggle" },
                            title: i18n::t(locale, label),
                            aria_label: i18n::t(locale, label),
                            aria_expanded: active_view == target,
                            aria_controls: "live-camera-controls",
                            disabled: !can_save,
                            onclick: move |_| ctx.view.clone().set(if active_view == target { View::LiveVideo } else { target }),
                            Icon { name: icon, size: 15 }
                        }
                    }
                }
                if gate.ptz {
                    DragPanButton { addr, creds, speed: ptz_speed, preview: pan_preview, enabled: can_save }
                }
                button {
                    class: "icon-btn live-video-save",
                    disabled: !can_save,
                    title: if can_save { i18n::t(locale, "snapshot_save") } else { i18n::t(locale, "snapshot_save_no_image") },
                    onclick: move |_| {
                        let Some(token) = profile_sig.read().clone() else { return };
                        let addr = addr.read().clone();
                        let creds = creds.read().clone();
                        let toast_ctx = ctx;
                        let directory = crate::persist::snapshot_directory(ctx.snapshot_dir.read().as_deref());
                        let name = format!("{}-{token}", crate::util::extract_ip(&addr));
                        let saved_label = i18n::t(locale, "snapshot_saved").to_string();
                        let failed_label = i18n::t(locale, "snapshot_save_failed").to_string();
                        spawn(async move {
                            let bytes = match crate::api::get_snapshot_uri(&addr, &creds, &token).await {
                                Ok(s) => {
                                    let url = crate::api::resolve_snapshot_url(&addr, &s.uri);
                                    match crate::api::fetch_snapshot_data_uri(&url, &creds).await {
                                        Ok(uri) => crate::util::decode_jpeg_data_uri(&uri),
                                        Err(e) => {
                                            toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}"));
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::info!(error = %e, "no ONVIF snapshot URI; decoding a key frame from RTSP");
                                    match video::rtsp::snapshot_jpeg(&addr, &token, &creds).await {
                                        Ok(b) => Some(b),
                                        Err(e2) => {
                                            toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}; RTSP: {e2}"));
                                            return;
                                        }
                                    }
                                }
                            };
                            match bytes {
                                Some(bytes) => match crate::util::save_snapshot(&directory, &name, &bytes) {
                                    Ok(path) => {
                                        tracing::info!(path = %path.display(), bytes = bytes.len(), "live snapshot saved");
                                        toast_ctx.push_toast(crate::state::ToastLevel::Success, format!("{}: {}", saved_label, path.display()));
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, directory = %directory.display(), "live snapshot save failed");
                                        toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}"));
                                    }
                                },
                                None => toast_ctx.push_toast(crate::state::ToastLevel::Error, failed_label),
                            }
                        });
                    },
                    Icon { name: "camera", size: 16 }
                }
                if is_recording {
                    span { class: "live-video-rec-time",
                        {format_elapsed(rec_elapsed.read().unwrap_or(0))}
                    }
                }
                button {
                    class: if is_recording { "icon-btn live-video-record live-video-record--on" } else { "icon-btn live-video-record" },
                    disabled: !can_record && !is_recording,
                    title: if is_recording { i18n::t(locale, "record_stop") } else { i18n::t(locale, "record_start") },
                    onclick: move |_| {
                        let id = stream_id.read().clone();
                        let toast_ctx = ctx;
                        let directory = crate::persist::recording_directory(ctx.recording_dir.read().as_deref());
                        let saved_label = i18n::t(locale, "record_saved").to_string();
                        let failed_label = i18n::t(locale, "record_failed").to_string();
                        let base = format!(
                            "{}-{}",
                            crate::util::sanitize_filename(&crate::util::extract_ip(&addr.read())),
                            crate::util::sanitize_filename(profile_sig.read().as_deref().unwrap_or("live")),
                        );
                        spawn(async move {
                            if video::rtsp::recording(&id).is_some() {
                                match video::rtsp::stop_recording(&id).await {
                                    Ok(st) => toast_ctx.push_toast(crate::state::ToastLevel::Success, format!("{}: {}", saved_label, st.path.display())),
                                    Err(e) => toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}")),
                                }
                                rec_elapsed.set(None);
                            } else {
                                match video::rtsp::start_recording(&id, &directory, &base) {
                                    Ok(_) => rec_elapsed.set(Some(0)),
                                    Err(e) => toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}")),
                                }
                            }
                        });
                    },
                    Icon { name: rec_icon, size: 16 }
                }
                button {
                    class: "icon-btn live-video-theater",
                    disabled: !can_save,
                    title: i18n::t(locale, "theater_open"),
                    aria_label: i18n::t(locale, "theater_open"),
                    onclick: move |_| {
                        menu_open.set(false);
                        theater.set(true);
                    },
                    Icon { name: "maximize", size: 16 }
                }
                div {
                    class: "live-menu",
                    onkeydown: move |event| {
                        if event.key() == Key::Escape && *menu_open.peek() {
                            event.prevent_default();
                            event.stop_propagation();
                            menu_open.set(false);
                        }
                    },
                    button {
                        class: "icon-btn",
                        title: i18n::t(locale, "workspace_more"),
                        aria_label: i18n::t(locale, "workspace_more"),
                        aria_expanded: *menu_open.read(),
                        aria_controls: "live-more-options",
                        onclick: move |_| menu_open.toggle(),
                        Icon { name: "more-horizontal", size: 16 }
                    }
                    if *menu_open.read() {
                        button {
                            class: "live-menu-overlay",
                            tabindex: "-1",
                            aria_label: i18n::t(locale, "btn_close"),
                            onmousedown: move |_| menu_open.set(false),
                        }
                        div { class: "live-menu-body", id: "live-more-options",
                            button {
                                class: "live-menu-item",
                                disabled: !can_save,
                                onclick: move |_| {
                                    menu_open.set(false);
                                    let Some(token) = profile_sig.read().clone() else { return };
                                    let addr = addr.read().clone();
                                    let creds = creds.read().clone();
                                    let backend_name = *backend_id.read();
                                    let toast_ctx = ctx;
                                    let failed_label = i18n::t(locale, "live_video_error").to_string();
                                    let title = ctx
                                        .selected
                                        .read()
                                        .and_then(|i| ctx.devices.read().get(i).map(|d| d.name.clone()))
                                        .unwrap_or_else(|| crate::util::extract_ip(&addr));
                                    spawn(async move {
                                        let backend = match backend_name {
                                            "rtsp" => video::rtsp(),
                                            _ => video::mjpeg(),
                                        };
                                        let Some(backend) = backend else { return };
                                        match backend.open(&addr, &token, &creds).await {
                                            Ok(src) => crate::views::pip::open(src, title, locale),
                                            Err(e) => toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}")),
                                        }
                                    });
                                },
                                Icon { name: "pip", size: 16 }
                                {i18n::t(locale, "pip_open")}
                            }
                            DevicePanel { gate, on_navigate: move |_| menu_open.set(false) }
                            div { class: "live-menu-playback",
                                span { {i18n::t(locale, "workspace_playback")} }
                                LiveModeTabs { mode }
                                if let Some(name) = backend_display {
                                    span { class: "live-video-backend", "{name}" }
                                }
                            }
                        }
                    }
                }
            }

            div { class: if controls_open { "live-workbench live-workbench--controls" } else { "live-workbench" },
                div { class: "live-feed",
                    ondoubleclick: move |_| {
                        // Fullscreen implies theater; exiting restores the
                        // theater state from before fullscreen.
                        let window = dioxus::desktop::window();
                        if window.window.fullscreen().is_none() {
                            pre_fullscreen_theater.set(theater());
                            theater.set(true);
                            window.set_fullscreen(true);
                        } else {
                            window.set_fullscreen(false);
                            theater.set(*pre_fullscreen_theater.peek());
                            pre_fullscreen_theater.set(false);
                        }
                    },
                    onmousedown: move |event| {
                        // Only the primary button drags the window; the
                        // right button opens the camera switcher, and its
                        // mouseup lands on the menu overlay — dragging on
                        // it would leave the anchor stuck on.
                        if event.data().trigger_button()
                            != Some(dioxus::html::input_data::MouseButton::Primary)
                        {
                            return;
                        }
                        if !theater() || window.window.fullscreen().is_some() {
                            return;
                        }
                        let c = event.data().screen_coordinates();
                        let Ok(pos) = window.window.outer_position() else { return };
                        let pos = pos.to_logical(window.window.scale_factor());
                        drag_anchor.set(Some((c.x, c.y, pos.x, pos.y)));
                    },
                    onmousemove: move |event| {
                        let Some((ax, ay, wx, wy)) = *drag_anchor.peek() else { return };
                        let c = event.data().screen_coordinates();
                        drag_window.window.set_outer_position(dioxus::desktop::LogicalPosition::new(wx + c.x - ax, wy + c.y - ay));
                    },
                    onmouseup: move |_| drag_anchor.set(None),
                    LiveVideoStage {
                        addr,
                        creds,
                        backend_id: Some(backend_id.into()),
                    }
                    if gate.ptz {
                        PanPositionOverlay { key: "{profile_key}", addr, creds, preview: pan_preview }
                    }
                }
                if controls_open {
                    aside { class: "live-controls", id: "live-camera-controls",
                        button {
                            class: "icon-btn live-controls-close",
                            title: i18n::t(locale, "workspace_close_controls"),
                            aria_label: i18n::t(locale, "workspace_close_controls"),
                            onclick: move |_| ctx.view.clone().set(View::LiveVideo),
                            Icon { name: "x", size: 16 }
                        }
                        if active_view == View::PtzControl {
                            PtzControlView { key: "{profile_key}", addr, creds, speed: ptz_speed }
                        } else {
                            ImagingView { key: "{profile_key}", addr, creds, show_encoder: false }
                        }
                    }
                }
                if theater_open {
                    button {
                        class: "theater-exit",
                        title: i18n::t(locale, "theater_exit"),
                        aria_label: i18n::t(locale, "theater_exit"),
                        // This button's job is to leave theater — always.
                        // (Restoring pre_fullscreen_theater here made it a
                        // no-op whenever fullscreen was entered from
                        // theater.)
                        onclick: move |_| {
                            dioxus::desktop::window().set_fullscreen(false);
                            theater.set(false);
                        },
                        Icon { name: "minimize", size: 16 }
                    }
                }
                if theater_open {
                    if let Some((mx, my)) = switch_pos {
                        ContextMenu {
                            x: mx,
                            y: my,
                            on_close: move |_| switch_menu.set(None),
                            // One entry per device, the active one
                            // check-marked. Clicking only moves the
                            // selection: the view remounts keyed by address
                            // and the global theater signal keeps theater
                            // mode on.
                            for (i, dev) in devices.iter().enumerate() {
                                button {
                                    key: "{i}",
                                    class: "ctx-menu-item",
                                    onclick: move |_| {
                                        switch_menu.set(None);
                                        ctx.selected.clone().set(Some(i));
                                    },
                                    span {
                                        class: "ctx-menu-item-icon",
                                        if sel == Some(i) {
                                            Icon { name: "check", size: 14 }
                                        } else {
                                            Icon { name: "video", size: 14 }
                                        }
                                    }
                                    "{dev.name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_elapsed(secs: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60
    )
}

#[component]
fn ModeTab(
    active: bool,
    label: &'static str,
    title: &'static str,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "live-video-mode-tab live-video-mode-tab--active"
    } else {
        "live-video-mode-tab"
    };
    rsx! {
        button {
            class,
            title,
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

/// Embeddable video stage — header-less, fills its parent.
///
/// `backend` is optional. `None` is the implicit default (current
/// installed backend = MJPEG); embedded users that want a specific
/// backend pass `Some(...)`.
#[component]
pub fn LiveVideoStage(
    addr: ReadSignal<String>,
    creds: Memo<Credentials>,
    /// Which backend to use as a reactive signal — reading it inside
    /// `use_resource` makes Dioxus re-run the fetch on tab switch.
    /// `None` falls back to the implicit default (MJPEG via
    /// `video::current()`). Signal-of-string instead of trait object
    /// because Dioxus props need PartialEq and trait objects don't
    /// implement it.
    #[props(optional)]
    backend_id: Option<ReadSignal<&'static str>>,
) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let profile_sig = ctx.selected_profile;

    let mut source = use_resource(move || {
        let addr = addr.read().clone();
        let creds = creds.read().clone();
        let profile = profile_sig.read().clone();
        // Read INSIDE the closure so changes re-trigger the resource.
        let backend_name = backend_id.map(|sig| *sig.read());
        async move {
            if addr.is_empty() {
                return Err("no_device".to_string());
            }
            let token = profile.ok_or_else(|| "no_profile".to_string())?;
            let backend = match backend_name {
                Some("mjpeg") => video::mjpeg(),
                Some("rtsp") => video::rtsp(),
                _ => video::current(),
            }
            .ok_or_else(|| "no_backend".to_string())?;
            backend
                .open(&addr, &token, &creds)
                .await
                .map_err(|e| format!("backend_error:{e}"))
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let retry = matches!(&*source.peek(), Some(Err(reason)) if reason.starts_with("backend_error:"));
            if retry {
                source.restart();
            }
        }
    });

    rsx! {
        div { class: "live-video-stage",
            match &*source.read_unchecked() {
                None => rsx! {
                    div { class: "live-video-placeholder",
                        {i18n::t(locale, "loading")}
                    }
                },
                Some(Err(reason)) => {
                    let key = match reason.as_str() {
                        "no_device"  => "live_video_no_device",
                        "no_profile" => "live_video_no_profile",
                        "no_backend" => "live_video_no_backend",
                        _            => "live_video_error",
                    };
                    let detail = reason
                        .strip_prefix("backend_error:")
                        .map(str::to_string);
                    rsx! {
                        div { class: "live-video-placeholder",
                            Icon { name: "alert-triangle", size: 28 }
                            p { {i18n::t(locale, key)} }
                            if let Some(msg) = detail {
                                p { class: "live-video-detail", "{msg}" }
                                button {
                                    class: "icon-btn",
                                    title: i18n::t(locale, "btn_retry"),
                                    aria_label: i18n::t(locale, "btn_retry"),
                                    onclick: move |_| source.restart(),
                                    Icon { name: "refresh-cw", size: 18 }
                                }
                            }
                        }
                    }
                }
                Some(Ok(src)) => rsx! { VideoPlayer { key: "{src.id}", source: src.clone(), locale } },
            }
        }
    }
}

#[component]
pub fn VideoPlayer(source: video::VideoSource, locale: crate::state::Locale) -> Element {
    let mut image_attempt = use_signal(|| 0_u64);
    let mut show_details = use_signal(|| *video::SHOW_VIDEO_DETAILS.borrow());
    use_future(move || async move {
        let mut updates = video::SHOW_VIDEO_DETAILS.subscribe();
        loop {
            show_details.set(*updates.borrow_and_update());
            if updates.changed().await.is_err() {
                break;
            }
        }
    });
    match source.embed {
        EmbedKind::Img | EmbedKind::SoftwareMjpeg => {
            let software = source.embed == EmbedKind::SoftwareMjpeg;
            let (label, hint) = if software {
                (
                    "video_decode_software_mjpeg",
                    "video_decode_software_mjpeg_hint",
                )
            } else {
                ("video_decode_snapshots", "video_decode_snapshots_hint")
            };
            let hint = if software {
                video::software_decode_hint(locale)
            } else {
                i18n::t(locale, hint).to_string()
            };
            rsx! {
                div { class: "live-video-frame live-video-frame--image",
                    "data-video-details": "{show_details}",
                    img {
                        key: "{source.url}:{image_attempt}",
                        class: "live-video-frame",
                        src: "{source.url}",
                        alt: i18n::t(locale, "nav_live_video"),
                        onerror: move |_| {
                            spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                image_attempt += 1;
                            });
                        },
                    }
                    div {
                        class: if software { "video-decode-status video-decode-status--software" } else { "video-decode-status" },
                        span {
                            role: "status",
                            title: "{hint}",
                            {i18n::t(locale, label)}
                        }
                    }
                }
            }
        }
        EmbedKind::Stream => rsx! {
            div {
                class: "live-video-frame live-video-frame--stream",
                "data-video-details": "{show_details}",
                dangerous_inner_html: video::stream_element_html(&source.url, locale),
            }
        },
    }
}
