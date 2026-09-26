#![allow(non_snake_case)]
use crate::components::Icon;
use crate::i18n;
use crate::state::{Credentials, Ctx};
use crate::video::{self, EmbedKind};
use dioxus::prelude::*;

/// Which video backend a stage should use for the current view.
/// Defaults to Rtsp — real frame rate + audio through the in-process
/// pipeline. Snapshot (MJPEG polling) stays one tab away for cameras
/// whose RTSP is broken or blocked. Preference is per-session
/// (intentionally not persisted yet — most users will pick once and
/// stay).
///
/// Reused by Imaging and PTZ so they can offer the same Snapshot/RTSP
/// choice as the dedicated Live Video view.
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

/// Reusable `<Snapshot | RTSP>` tab strip. The caller owns the `mode`
/// signal and decides where to place this in its header. Designed to drop
/// into Live Video, Imaging, and PTZ uniformly so users encounter the same
/// affordance everywhere.
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

/// Live video panel — full view with header.
///
/// Tab strip lets the user choose between Snapshot mode (the
/// always-available MJPEG polling backend, ~5–10 fps) and RTSP mode
/// (native RTSP client → WebCodecs, real frame rate + audio).
///
/// `LiveVideoStage` is reusable elsewhere (Imaging preview), but
/// embedded uses pin to Snapshot — the tab strip lives here only.
#[component]
pub fn LiveVideoView(addr: ReadSignal<String>, creds: Memo<Credentials>) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let mode = use_signal(LiveVideoMode::default);
    let profile_sig = ctx.selected_profile;

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

    rsx! {
        div { class: "live-video-view",
            div { class: "content-header",
                Icon { name: "video", size: 20 }
                span { class: "content-title", {i18n::t(locale, "nav_live_video")} }
                LiveModeTabs { mode }
                if let Some(name) = backend_display {
                    span { class: "live-video-backend",
                        " · {name}"
                    }
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
                        let default_name = format!("{}.jpg", crate::util::sanitize_filename(&token));
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
                            let Some(handle) = rfd::AsyncFileDialog::new()
                                .set_file_name(&default_name)
                                .add_filter("JPEG", &["jpg", "jpeg"])
                                .save_file()
                                .await
                            else {
                                return;
                            };
                            let path = handle.path().to_path_buf();
                            match bytes {
                                Some(bytes) => match std::fs::write(&path, &bytes) {
                                    Ok(()) => {
                                        tracing::info!(path = %path.display(), bytes = bytes.len(), "live snapshot saved");
                                        toast_ctx.push_toast(crate::state::ToastLevel::Success, format!("{}: {}", saved_label, path.display()));
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, path = %path.display(), "live snapshot save failed");
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
                                let dir = dirs::video_dir()
                                    .or_else(dirs::home_dir)
                                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                                    .join("OxDM");
                                match video::rtsp::start_recording(&id, &dir, &base) {
                                    Ok(_) => rec_elapsed.set(Some(0)),
                                    Err(e) => toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}")),
                                }
                            }
                        });
                    },
                    Icon { name: rec_icon, size: 16 }
                }
                button {
                    class: "icon-btn live-video-pip",
                    disabled: !can_save,
                    title: i18n::t(locale, "pip_open"),
                    onclick: move |_| {
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
                                Ok(src) => crate::views::pip::open(src, title),
                                Err(e) => toast_ctx.push_toast(crate::state::ToastLevel::Error, format!("{failed_label}: {e}")),
                            }
                        });
                    },
                    Icon { name: "pip", size: 16 }
                }
            }

            LiveVideoStage {
                addr,
                creds,
                backend_id: Some(backend_id.into()),
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
/// backend pass `Some(...)`. Re-using this from Imaging keeps the
/// preview consistent with Live Video.
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

    let source = use_resource(move || {
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
                            }
                        }
                    }
                }
                Some(Ok(src)) => match src.embed {
                    EmbedKind::Img => rsx! {
                        img {
                            class: "live-video-frame",
                            src: "{src.url}",
                            alt: "live video stream"
                        }
                    },
                    EmbedKind::Stream => rsx! {
                        div {
                            class: "live-video-frame live-video-frame--stream",
                            dangerous_inner_html: video::stream_element_html(&src.url),
                        }
                    },
                },
            }
        }
    }
}
