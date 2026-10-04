#![allow(non_snake_case)]
use dioxus::prelude::*;

mod api;
mod components;
mod device_ops;
mod i18n;
mod mock_servers;
mod persist;
mod sessions;
mod state;
#[cfg(test)]
mod tests;
pub(crate) mod util;
mod video;
mod views;

use components::{ConfirmDialogModal, DeviceList, ToastContainer};
use state::{Credentials, Ctx, GlobalKey, SettingsTab, View};
use views::MainContent;

/// CSS is embedded directly in the binary so the release ships as a
/// single executable — no sibling `assets/` directory needed at runtime.
pub(crate) const MAIN_CSS: &str = include_str!("../assets/main.css");
/// `<oxdm-stream>` — the WebCodecs player element for the native RTSP
/// backend. See video/rtsp.rs for the wire format it consumes.
pub(crate) const STREAM_JS: &str = include_str!("../assets/js/oxdm-stream.js");

/// App icon — same master PNG `build.rs` uses to mint the embedded ICO,
/// re-decoded here at startup so `WindowBuilder::with_window_icon` has
/// something to feed to the OS title bar / taskbar / Alt-Tab switcher.
/// `dx` doesn't wire this up for us; without it, Windows + Wry falls back
/// to the Dioxus default.
const ICON_PNG: &[u8] = include_bytes!("../assets/icons/icon.png");

/// Decode the embedded PNG into a `tao::window::Icon`. Returns `None` if
/// the PNG is in an unexpected colour format — the rest of the app keeps
/// working with the default icon.
pub(crate) fn load_window_icon() -> Option<dioxus::desktop::tao::window::Icon> {
    let decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(buf.len() / 3 * 4);
            for chunk in buf.as_chunks::<3>().0 {
                out.extend_from_slice(chunk);
                out.push(0xFF);
            }
            out
        }
        _ => return None,
    };
    dioxus::desktop::tao::window::Icon::from_rgba(rgba, info.width, info.height).ok()
}

/// Set up tracing with a stderr layer (env-filter respected). When
/// `log_to_file` is true, also adds a daily-rolling file appender at
/// `~/.oxdm/logs/oxdm.log.*` and returns its `WorkerGuard` — the caller
/// MUST keep it alive (bind it for the duration of `main`) so the
/// background flush thread isn't dropped before the program exits.
///
/// Defaults to off because most users never look at logs but they do
/// notice ~/.oxdm growing on disk. The About dialog has the toggle and
/// it persists to config.toml; takes effect on next launch.
fn init_logging(log_to_file: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let env_filter =
        || EnvFilter::try_from_default_env().unwrap_or_else(|_| "oxdm=info".parse().unwrap());
    let stderr_layer = fmt::layer().with_target(false).with_filter(env_filter());

    let (file_layer, guard) = if log_to_file {
        let log_dir = dirs::home_dir().map(|h| h.join(".oxdm").join("logs"));
        match log_dir {
            Some(ref dir) => match std::fs::create_dir_all(dir) {
                Ok(()) => {
                    let file_appender = tracing_appender::rolling::daily(dir, "oxdm.log");
                    let (nb, guard) = tracing_appender::non_blocking(file_appender);
                    let layer = fmt::layer()
                        .with_writer(nb)
                        .with_ansi(false)
                        .with_target(false)
                        .with_filter(env_filter());
                    (Some(layer), Some(guard))
                }
                Err(e) => {
                    eprintln!("Could not create log dir {}: {e}", dir.display());
                    (None, None)
                }
            },
            None => (None, None),
        }
    } else {
        (None, None)
    };

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    guard
}

/// Path to the directory holding rotated log files.
pub fn log_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".oxdm").join("logs"))
}

#[cfg(target_os = "linux")]
fn hyprland_session(signature: Option<&str>, desktop: Option<&str>) -> bool {
    signature.is_some_and(|value| !value.trim().is_empty())
        || desktop.is_some_and(|value| {
            value
                .split(':')
                .any(|name| name.trim().eq_ignore_ascii_case("Hyprland"))
        })
}

fn main_window_builder(hyprland: bool) -> dioxus::desktop::WindowBuilder {
    dioxus::desktop::WindowBuilder::new()
        .with_title("OxDM")
        .with_window_icon(load_window_icon())
        .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 800.0))
        .with_min_inner_size(main_window_min_size(false))
        // Dioxus also drops its default Window/Edit menu for undecorated windows.
        .with_decorations(!hyprland)
}

pub(crate) fn main_window_min_size(theater: bool) -> dioxus::desktop::LogicalSize<f64> {
    if theater {
        dioxus::desktop::LogicalSize::new(320.0, 180.0)
    } else {
        dioxus::desktop::LogicalSize::new(900.0, 500.0)
    }
}

pub(crate) fn main_window_layout_size(
    size: dioxus::desktop::LogicalSize<f64>,
) -> dioxus::desktop::LogicalSize<f64> {
    let min = main_window_min_size(false);
    dioxus::desktop::LogicalSize::new(size.width.max(min.width), size.height.max(min.height))
}

pub(crate) fn set_main_window_theater(
    window: &dioxus::desktop::tao::window::Window,
    theater: bool,
) {
    window.set_always_on_top(theater);
    window.set_min_inner_size(Some(main_window_min_size(theater)));
    if !theater {
        let size = window.inner_size().to_logical::<f64>(window.scale_factor());
        let restored = main_window_layout_size(size);
        if restored != size {
            window.set_inner_size(restored);
        }
    }
}

fn main() {
    // Read just the log preference up-front so init_logging knows whether
    // to spin up the file appender. The full config is re-loaded inside
    // App() — this duplicate read is one tiny TOML parse, not worth a
    // hand-off mechanism.
    let log_to_file = persist::load_config().log_to_file;
    let _log_guard = init_logging(log_to_file);

    tracing::info!(log_to_file, "OxDM starting");

    #[cfg(target_os = "linux")]
    let hyprland = hyprland_session(
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().as_deref(),
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
    );
    #[cfg(not(target_os = "linux"))]
    let hyprland = false;

    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus::desktop::Config::new().with_window(main_window_builder(hyprland)))
        .launch(App);
}

fn App() -> Element {
    let cfg = use_hook(persist::load_config);

    // Install both video backends. MJPEG binds the shared loopback server
    // and is the always-on fallback; the native RTSP backend (the default
    // live mode) serves through the same listener. Both run inside the
    // dioxus tokio runtime so spawning is safe here. Failure to bind only
    // logs; the rest of the app keeps working without live video.
    use_hook(|| match video::mjpeg::MjpegBackend::start() {
        Ok(b) => video::install_mjpeg(std::sync::Arc::new(b)),
        Err(e) => tracing::error!(error = %e, "failed to start MJPEG backend"),
    });
    use_hook(|| video::install_rtsp(std::sync::Arc::new(video::rtsp::RtspBackend)));

    let ctx = Ctx {
        devices: use_signal(Vec::new),
        selected: use_signal(|| None),
        view: use_signal(|| View::Welcome),
        settings_tab: use_signal(|| SettingsTab::Identification),
        scanning: use_signal(|| false),
        theme: use_signal(|| persist::theme_from_str(&cfg.theme)),
        locale: use_signal(|| persist::locale_from_str(&cfg.locale)),
        toasts: use_signal(Vec::new),
        next_toast_id: use_signal(|| 0),
        dialog: use_signal(|| None),
        global_credentials: use_signal(Credentials::default),
        health_groups: use_signal(Vec::new),
        health_list: use_signal(|| crate::state::HealthListSel::AllDevices),
        dragging: use_signal(Vec::new),
        drag_pending: use_signal(|| None),
        drag_just_finished: use_signal(|| false),
        selected_profile: use_signal(|| None),
        keyboard_action: use_signal(|| None),
        log_to_file: use_signal(|| cfg.log_to_file),
        tls_strict: use_signal(|| cfg.tls_strict),
        snapshot_dir: use_signal(|| cfg.snapshot_dir.clone()),
        recording_dir: use_signal(|| cfg.recording_dir.clone()),
        camera_item_size: use_signal(|| cfg.camera_item_size()),
        sidebar_collapsed: use_signal(|| cfg.sidebar_collapsed),
        ptz_speed: use_signal(|| cfg.ptz_speed()),
        loaded: use_signal(|| false),
    };
    // Seed the TLS-strict atomic from config so the first snapshot fetch
    // after launch already honours the saved preference.
    api::set_tls_strict(cfg.tls_strict);
    use_context_provider(|| ctx);

    // One keychain read for every credential. On macOS it blocks until the
    // user answers the access prompt, so it must not run inside render —
    // the window would stay blank until then.
    use_future(move || {
        let cfg = cfg.clone();
        async move {
            let result = tokio::task::spawn_blocking(move || {
                let (creds, map) = persist::load_all_credentials(&cfg);
                let devices = persist::load_devices(&map);
                let groups = persist::load_health_groups(&map);
                (creds, devices, groups)
            })
            .await;
            match result {
                Ok((creds, devices, groups)) => {
                    ctx.global_credentials.clone().set(creds);
                    ctx.devices.clone().set(devices);
                    ctx.health_groups.clone().set(groups);
                }
                Err(e) => tracing::error!(error = %e, "persisted-state load task failed"),
            }
            ctx.loaded.clone().set(true);
        }
    });

    // Auto-save application preferences when they change.
    // Also pushes tls_strict into the api atomic so a toggle takes effect
    // on the next snapshot without a restart (unlike log_to_file).
    use_effect(move || {
        let theme = *ctx.theme.read();
        let locale = *ctx.locale.read();
        let log_to_file = *ctx.log_to_file.read();
        let tls_strict = *ctx.tls_strict.read();
        let snapshot_dir = ctx.snapshot_dir.read().clone();
        let recording_dir = ctx.recording_dir.read().clone();
        let camera_item_size = *ctx.camera_item_size.read();
        let sidebar_collapsed = *ctx.sidebar_collapsed.read();
        let ptz_speed = *ctx.ptz_speed.read();
        api::set_tls_strict(tls_strict);
        persist::save_config(persist::ConfigOut {
            theme: persist::theme_to_str(theme).to_string(),
            locale: persist::locale_to_str(locale).to_string(),
            log_to_file,
            tls_strict,
            snapshot_dir,
            recording_dir,
            camera_item_size,
            sidebar_collapsed,
            ptz_speed,
        });
    });

    // Re-verify auth when credentials change
    use_effect(move || {
        let _creds = ctx.global_credentials.read();
        device_ops::reverify_auth(ctx, ctx.devices);
    });

    // Forget the selected profile when the selected *device* changes.
    //
    // Profile tokens are per-device and collide freely across brands — plenty
    // of cameras call theirs `Profile_1` / `MainStream`. Carrying one across a
    // device switch either resolves to a different camera's channel of the same
    // name, or misses and silently falls back to lens 0 (see
    // `api::pick_channel`). Neither is visible in the UI. Only `ctx.selected`
    // is subscribed: the profile selector sets `selected_profile` without
    // touching it, so a stream selection does not clear itself.
    //
    // Session restore hands its profile over through `restore_profile`, so the
    // clear that follows its device selection installs it instead of wiping it.
    let mut restore_profile: Signal<Option<String>> = use_signal(|| None);
    use_effect(move || {
        let _device_changed = *ctx.selected.read();
        ctx.selected_profile.clone().set(restore_profile.take());
    });

    // Restore the last session once its device is in the list. Manual devices
    // arrive from disk; discovered ones only after a scan, so this waits on
    // `devices` rather than running once. Dropped as soon as the user picks a
    // device themselves.
    let mut pending_session: Signal<Option<persist::SessionFile>> =
        use_signal(|| Some(persist::load_session()));
    use_effect(move || {
        let devices = ctx.devices.read();
        let Some(session) = pending_session.peek().clone() else {
            return;
        };
        if ctx.selected.peek().is_some() {
            pending_session.set(None);
            return;
        }
        let view = persist::view_from_str(&session.view);
        if matches!(view, View::AppSettings | View::HealthOverview) {
            ctx.view.clone().set(view);
        }
        if session.device_addr.is_empty() {
            pending_session.set(None);
            return;
        }
        let Some(idx) = devices.iter().position(|d| d.addr == session.device_addr) else {
            return;
        };
        pending_session.set(None);
        restore_profile.set(Some(session.profile).filter(|p| !p.is_empty()));
        ctx.settings_tab
            .clone()
            .set(persist::settings_tab_from_str(&session.settings_tab));
        ctx.view.clone().set(view);
        ctx.selected.clone().set(Some(idx));
    });

    use_effect(move || {
        let selected = *ctx.selected.read();
        let view = *ctx.view.read();
        let tab = *ctx.settings_tab.read();
        let profile = ctx.selected_profile.read().clone();
        if !*ctx.loaded.read() {
            return;
        }
        // A pending restore must not be overwritten by the launch defaults,
        // but a user selection made before it resolves wins over it.
        let pending = pending_session.peek().clone();
        if let Some(session) = pending {
            if selected.is_none()
                && (view != View::AppSettings || view == persist::view_from_str(&session.view))
            {
                return;
            }
            pending_session.set(None);
        }
        let devices = ctx.devices.peek();
        let device_addr = selected
            .and_then(|i| devices.get(i))
            .map(|d| d.addr.clone())
            .unwrap_or_default();
        persist::save_session(&persist::SessionFile {
            device_addr,
            profile: profile.unwrap_or_default(),
            view: persist::view_to_str(view).to_string(),
            settings_tab: persist::settings_tab_to_str(tab).to_string(),
        });
    });

    // Auto-save credentials + devices when either changes (single keychain
    // write). `groups` is `.peek()`d (included in the blob, not subscribed) so a
    // device/cred change re-emits group creds too and can't clobber them.
    use_effect(move || {
        if !*ctx.loaded.read() {
            return;
        }
        let creds = ctx.global_credentials.read().clone();
        let devices = ctx.devices.read().clone();
        let groups = ctx.health_groups.peek().clone();
        persist::save_credentials_and_devices(&creds, &devices, &groups);
    });

    // Auto-save health groups when they change. Symmetrically, `creds`/`devices`
    // are `.peek()`d so a group change re-emits the full keychain blob (device +
    // global creds included) — neither effect erases the other's keys.
    use_effect(move || {
        if !*ctx.loaded.read() {
            return;
        }
        let groups = ctx.health_groups.read().clone();
        let creds = ctx.global_credentials.peek().clone();
        let devices = ctx.devices.peek().clone();
        persist::save_health_groups(&creds, &devices, &groups);
    });

    let theme_class = ctx.theme.read().css_class();
    // While a pointer drag is active, force the grabbing cursor + suppress text
    // selection app-wide (CSS `.dragging-active`), so there's no stuck no-drop
    // cursor like native HTML5 DnD leaves on desktop.
    let dragging_class = if ctx.dragging.read().is_empty() {
        ""
    } else {
        " dragging-active"
    };
    // Squared px distance a press must travel before it becomes a drag (5px);
    // squared to avoid a sqrt on every pointermove.
    const DRAG_THRESHOLD_SQ: f64 = 25.0;

    rsx! {
        document::Style { {MAIN_CSS} }
        document::Script { {STREAM_JS} }
        ErrorBoundary {
            handle_error: |errors: ErrorContext| {
                rsx! {
                    div { class: "error-boundary",
                        h2 { "Something went wrong" }
                        for error in errors.error() {
                            p { class: "error-boundary-detail", "{error}" }
                        }
                        p { "Please restart the application." }
                    }
                }
            },
            div {
                class: "{theme_class}{dragging_class}",
                tabindex: "-1",
                autofocus: true,
                // Pointer-based drag-and-drop (native HTML5 DnD is broken on
                // desktop wry). Sources set `drag_pending` on pointerdown; here
                // at the root we promote it to an active drag once it moves, and
                // complete it on release via `finish_drag_at`'s hit-test.
                onpointermove: move |e: Event<PointerData>| {
                    if ctx.dragging.peek().is_empty() {
                        if let Some(p) = ctx.drag_pending.peek().clone() {
                            let c = e.data().client_coordinates();
                            let (dx, dy) = (c.x - p.start_x, c.y - p.start_y);
                            if dx * dx + dy * dy >= DRAG_THRESHOLD_SQ {
                                ctx.dragging.clone().set(p.payload);
                            }
                        }
                    }
                },
                onpointerup: move |e: Event<PointerData>| {
                    let active = !ctx.dragging.peek().is_empty();
                    ctx.drag_pending.clone().set(None);
                    if active {
                        let c = e.data().client_coordinates();
                        let label =
                            i18n::t(*ctx.locale.peek(), "hgroups_new_group").to_string();
                        ctx.drag_just_finished.clone().set(true);
                        ctx.finish_drag_at(c.x, c.y, label);
                    }
                },
                // App-level shortcuts. Esc is handled per-modal because each
                // dialog has its own close semantics; the keys here are the
                // ones that should always work no matter what's focused.
                onkeydown: move |evt| {
                    let key = evt.key();
                    let mods = evt.modifiers();
                    use dioxus::html::input_data::keyboard_types::{Key, Modifiers};
                    let ctrl_or_meta =
                        mods.contains(Modifiers::CONTROL) || mods.contains(Modifiers::META);
                    let mut action = ctx.keyboard_action;
                    match key {
                        Key::Character(ref s) if ctrl_or_meta && s.eq_ignore_ascii_case("f") => {
                            action.set(Some(GlobalKey::FocusSearch));
                            evt.prevent_default();
                        }
                        Key::F5 => {
                            action.set(Some(GlobalKey::Scan));
                            evt.prevent_default();
                        }
                        Key::ArrowUp => {
                            action.set(Some(GlobalKey::NavUp));
                            evt.prevent_default();
                        }
                        Key::ArrowDown => {
                            action.set(Some(GlobalKey::NavDown));
                            evt.prevent_default();
                        }
                        _ => {}
                    }
                },
                div { class: "shell-body",
                    DeviceList {}
                    MainContent {}
                }
                ToastContainer {}
                ConfirmDialogModal {}
                if !*ctx.loaded.read() {
                    div { class: "dialog-overlay",
                        div { class: "dialog",
                            div { class: "dialog-body",
                                div { class: "keychain-wait-row",
                                    span { class: "status-bar-spinner" }
                                    span { class: "dialog-title",
                                        {i18n::t(*ctx.locale.read(), "keychain_wait_title")}
                                    }
                                }
                                p { class: "dialog-hint",
                                    {i18n::t(*ctx.locale.read(), "keychain_wait_hint")}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
