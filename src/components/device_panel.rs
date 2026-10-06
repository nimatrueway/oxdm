#![allow(non_snake_case)]
use crate::api;
use crate::components::{ContextMenu, CtxMenuItem, Icon, RenameGroupDialog};
use crate::i18n;
use crate::state::{
    new_group_id, ConfirmDialog, Credentials, Ctx, HealthDeviceRef, HealthGroup, HealthListSel,
    SettingsTab, View, WorkspaceTab,
};
use dioxus::prelude::*;
use tracing::{debug, warn};

#[component]
pub fn DevicePanel(gate: api::DeviceGate, on_navigate: EventHandler<()>) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let active = ctx.view.read().workspace_tab(*ctx.settings_tab.read());

    rsx! {
        nav { class: "live-menu-navigation",
            for (tab, icon, label, target) in [
                (WorkspaceTab::Recordings, "clock", "nav_recordings", View::Recordings),
                (WorkspaceTab::Settings, "wrench", "nav_settings", View::DeviceSettings),
            ] {
                if tab != WorkspaceTab::Recordings || gate.recordings || active == Some(tab) {
                    button {
                        class: "live-menu-item",
                        title: i18n::t(locale, label),
                        aria_label: i18n::t(locale, label),
                        disabled: ctx.selected.read().is_none(),
                        onclick: move |_| {
                            ctx.settings_tab.clone().set(SettingsTab::Identification);
                            ctx.view.clone().set(target);
                            on_navigate.call(());
                        },
                        Icon { name: icon, size: 16 }
                        {i18n::t(locale, label)}
                    }
                }
            }
        }
    }
}

#[component]
pub fn ProfileSelector(addr: ReadSignal<String>, creds: Memo<Credentials>) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let mut profiles_res = use_resource(move || {
        let addr = addr.read().clone();
        let creds = creds.read().clone();
        async move {
            let mut profiles = api::get_profiles(&addr, &creds).await?;
            let hidden = crate::persist::load_hidden_profiles();
            profiles.retain(|profile| {
                profile.video_encoder_token.is_some()
                    && !hidden.contains(&crate::persist::hidden_profile_key(&addr, &profile.token))
            });
            Ok::<_, String>((addr, profiles))
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let retry = matches!(&*profiles_res.peek(), Some(Err(_)));
            if retry {
                profiles_res.restart();
            }
        }
    });

    use_effect(move || {
        let selected = ctx.selected_profile.read().clone();
        let next = match &*profiles_res.read() {
            Some(Ok((loaded_addr, profiles))) if loaded_addr == &*addr.read() => {
                crate::state::preferred_video_profile(profiles, selected.as_deref())
                    .map(|profile| profile.token.clone())
            }
            _ => return,
        };
        if selected != next {
            ctx.selected_profile.clone().set(next);
        }
    });

    let selected = ctx.selected_profile.read().clone();
    let addr_now = addr.read().clone();

    rsx! {
        div { class: "stream-selector",
            match &*profiles_res.read_unchecked() {
                None => rsx! { span { class: "stream-selector-message", {i18n::t(locale, "loading")} } },
                Some(Err(error)) => rsx! {
                    span { class: "stream-selector-error", title: "{error}", "{error}" }
                    button {
                        class: "icon-btn",
                        title: i18n::t(locale, "btn_retry"),
                        aria_label: i18n::t(locale, "btn_retry"),
                        onclick: move |_| profiles_res.restart(),
                        Icon { name: "refresh-cw", size: 15 }
                    }
                },
                Some(Ok((loaded_addr, _))) if loaded_addr != &addr_now => rsx! {
                    span { class: "stream-selector-message", {i18n::t(locale, "loading")} }
                },
                Some(Ok((_, profiles))) => {
                    let current = crate::state::preferred_video_profile(profiles, selected.as_deref());
                    let current_token = current.map(|profile| profile.token.clone()).unwrap_or_default();
                    let current_source = current.and_then(|profile| profile.video_source_token.clone()).unwrap_or_default();
                    let mut sources = Vec::new();
                    for profile in profiles {
                        let source = profile.video_source_token.clone().unwrap_or_default();
                        if !sources.contains(&source) {
                            sources.push(source);
                        }
                    }
                    let source_profiles = profiles.clone();
                    rsx! {
                        if profiles.is_empty() {
                            span { class: "stream-selector-message", {i18n::t(locale, "no_profiles")} }
                        } else {
                            if sources.len() > 1 {
                                label { class: "stream-field",
                                    span { {i18n::t(locale, "workspace_lens")} }
                                    select {
                                        class: "stream-select",
                                        value: current_source.clone(),
                                        onchange: move |event| {
                                            let source = event.value();
                                            if let Some(profile) = source_profiles.iter().find(|profile| profile.video_source_token.as_deref().unwrap_or_default() == source) {
                                                ctx.selected_profile.clone().set(Some(profile.token.clone()));
                                            }
                                        },
                                        for source in sources {
                                            option {
                                                value: source.clone(),
                                                if source.is_empty() { {i18n::t(locale, "workspace_unassigned")} } else { "{source}" }
                                            }
                                        }
                                    }
                                }
                            }
                            label { class: "stream-field",
                                span { {i18n::t(locale, "workspace_stream")} }
                                select {
                                    class: "stream-select",
                                    value: current_token,
                                    onchange: move |event| ctx.selected_profile.clone().set(Some(event.value())),
                                    for profile in profiles.iter().filter(|profile| profile.video_source_token.as_deref().unwrap_or_default() == current_source) {
                                        option {
                                            value: profile.token.clone(),
                                            if profile.name.is_empty() { "{profile.token}" } else { "{profile.name}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// Health mode's sidebar: "All devices" + one entry per saved group.
/// Clicking sets `ctx.health_list` (the Health Overview reads it); right-click
/// a group to rename / delete.
#[component]
pub fn HealthGroupsPanel() -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let mut ctx_menu: Signal<Option<(f64, f64, String)>> = use_signal(|| None);
    let rename_open = use_signal(|| false);
    let mut rename_id = use_signal(String::new);

    let mut new_name = use_signal(String::new);

    let groups = ctx.health_groups.read().clone();
    let current = ctx.health_list.read().clone();
    let is_all = matches!(current, HealthListSel::AllDevices);
    let is_dragging = !ctx.dragging.read().is_empty();

    let go = move |sel: HealthListSel| ctx.health_list.clone().set(sel);

    // Dedupe-append helper.
    fn push_deduped(devices: &mut Vec<HealthDeviceRef>, r: HealthDeviceRef) {
        let dup = devices
            .iter()
            .any(|x| (!r.endpoint.is_empty() && x.endpoint == r.endpoint) || x.addr == r.addr);
        if !dup {
            devices.push(r);
        }
    }

    // Create a group from the typed name (or an auto-name), optionally seeding
    // it with the dragged devices; then select it.
    let create_group = use_callback(move |with_dragged: bool| {
        let refs = if with_dragged {
            ctx.dragging.peek().clone()
        } else {
            Vec::new()
        };
        ctx.dragging.clone().set(Vec::new());
        let mut hg = ctx.health_groups;
        let new_id = {
            let mut groups = hg.write();
            let typed = new_name.peek().trim().to_string();
            let name = if typed.is_empty() {
                format!(
                    "{} {}",
                    i18n::t(locale, "hgroups_new_group"),
                    groups.len() + 1
                )
            } else {
                typed
            };
            let id = new_group_id(&groups);
            let mut devices = Vec::new();
            for r in refs {
                push_deduped(&mut devices, r);
            }
            groups.push(HealthGroup {
                id: id.clone(),
                name,
                devices,
                ..Default::default()
            });
            id
        };
        new_name.set(String::new());
        ctx.health_list.clone().set(HealthListSel::Group(new_id));
    });

    rsx! {
        div { class: "health-groups-panel",
            div { class: "panel-section",
                button {
                    class: if is_all { "group-sb-item group-sb-item--active" } else { "group-sb-item" },
                    onclick: move |_| go(HealthListSel::AllDevices),
                    span { class: "group-sb-icon", Icon { name: "activity", size: 14 } }
                    {i18n::t(locale, "hgroups_all_devices")}
                }
            }

            div { class: "panel-section",
                div { class: "panel-section-title", {i18n::t(locale, "devtab_groups")} }
                if groups.is_empty() {
                    div { class: "sidebar-groups-hint", {i18n::t(locale, "hgroups_add_hint")} }
                } else {
                    for (i , g) in groups.iter().enumerate() {
                        button {
                            key: "{i}",
                            class: {
                                let active = matches!(&current, HealthListSel::Group(id) if *id == g.id);
                                let drop = if is_dragging { " group-sb-item--droppable" } else { "" };
                                if active { format!("group-sb-item group-sb-item--active{drop}") } else { format!("group-sb-item{drop}") }
                            },
                            onclick: {
                                let gid = g.id.clone();
                                move |_| go(HealthListSel::Group(gid.clone()))
                            },
                            oncontextmenu: {
                                let gid = g.id.clone();
                                move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    let c = e.data().client_coordinates();
                                    ctx_menu.set(Some((c.x, c.y, gid.clone())));
                                }
                            },
                            // Drop target marker for the pointer-drag hit-test
                            // (Ctx::finish_drag_at → elementFromPoint on pointerup).
                            "data-drop-group": g.id.clone(),
                            span { class: "group-sb-icon", Icon { name: "folder", size: 14 } }
                            span { class: "group-sb-name", {format!("{} ({})", g.name, g.devices.len())} }
                        }
                    }
                }
                div {
                    class: if is_dragging { "hgroups-new-row hgroups-drop-zone" } else { "hgroups-new-row" },
                    "data-drop-newgroup": "1",
                    input {
                        class: "form-input form-input--flex",
                        r#type: "text",
                        placeholder: i18n::t(locale, "hgroups_new_group_placeholder"),
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                    button {
                        class: "btn btn-sm btn-primary",
                        onclick: move |_| create_group.call(false),
                        Icon { name: "plus", size: 14 }
                    }
                }
            }
        }

        if let Some((mx, my, gid)) = ctx_menu.read().clone() {
            ContextMenu {
                x: mx,
                y: my,
                on_close: move |_| ctx_menu.set(None),
                CtxMenuItem {
                    icon: "edit-2",
                    label: i18n::t(locale, "hgroups_rename"),
                    on_click: {
                        let gid = gid.clone();
                        move |_| {
                            ctx_menu.set(None);
                            rename_id.set(gid.clone());
                            rename_open.clone().set(true);
                        }
                    },
                }
                CtxMenuItem {
                    icon: "trash-2",
                    label: i18n::t(locale, "hgroups_delete"),
                    danger: true,
                    on_click: {
                        let gid = gid.clone();
                        move |_| {
                            ctx_menu.set(None);
                            let gid = gid.clone();
                            let name = ctx
                                .health_groups
                                .peek()
                                .iter()
                                .find(|g| g.id == gid)
                                .map(|g| g.name.clone())
                                .unwrap_or_default();
                            ctx.dialog.clone().set(Some(ConfirmDialog {
                                title: i18n::t(locale, "hgroups_delete").to_string(),
                                message: i18n::t(locale, "hgroups_delete_confirm").replace("{name}", &name),
                                confirm_label: i18n::t(locale, "btn_confirm").to_string(),
                                cancel_label: i18n::t(locale, "btn_cancel").to_string(),
                                dangerous: true,
                                on_confirm: EventHandler::new(move |_| {
                                    ctx.health_groups.clone().write().retain(|g| g.id != gid);
                                    if matches!(&*ctx.health_list.peek(), HealthListSel::Group(id) if *id == gid) {
                                        ctx.health_list.clone().set(HealthListSel::AllDevices);
                                    }
                                }),
                            }));
                        }
                    },
                }
            }
        }

        RenameGroupDialog { open: rename_open, group_id: rename_id.read().clone() }
    }
}

// ── NVT Profile Thumbnails ──────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct ProfileInfo {
    profile_token: String,
    profile_name: String,
    /// None when GetSnapshotUri is not supported by the device for this profile.
    snapshot_url: Option<String>,
    /// `true` when the camera marked the snapshot URI as single-use
    /// (`<tt:InvalidAfterConnect>true</tt:InvalidAfterConnect>`). Each tick
    /// must re-resolve a fresh URL via GetSnapshotUri instead of reusing the
    /// cached one — seen on GeoVision LPR cameras whose URLs embed a
    /// per-call timestamp pointing at a temp file the camera then deletes.
    invalid_after_connect: bool,
    creds: Credentials,
    /// Camera-side `fixed` flag: DeleteProfile is refused, so "delete" hides.
    fixed: bool,
    /// Bound to a video encoder — an RTSP key frame can stand in for a
    /// missing snapshot URI.
    has_video: bool,
    hidden: bool,
}

#[component]
pub fn ProfileThumbnails(gate: api::DeviceGate) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();

    let mut profiles_res = use_resource(move || {
        let devices = ctx.devices.read();
        let selected = *ctx.selected.read();
        let dev = selected.and_then(|i| devices.get(i)).cloned();
        let creds = dev
            .as_ref()
            .map(|d| ctx.credentials_for(d))
            .unwrap_or_else(|| ctx.global_credentials.read().clone());
        let addr = dev.map(|d| d.addr).unwrap_or_default();

        async move {
            if addr.is_empty() {
                return Ok::<(String, Vec<ProfileInfo>), String>((addr, Vec::new()));
            }

            let profiles = api::get_profiles(&addr, &creds).await?;
            debug!(addr = %addr, count = profiles.len(), "GetProfiles OK");
            let hidden_set = crate::persist::load_hidden_profiles();

            let mut infos = Vec::new();
            for profile in &profiles {
                let hidden =
                    hidden_set.contains(&crate::persist::hidden_profile_key(&addr, &profile.token));
                let base = ProfileInfo {
                    profile_token: profile.token.clone(),
                    profile_name: profile.name.clone(),
                    snapshot_url: None,
                    invalid_after_connect: false,
                    creds: creds.clone(),
                    fixed: profile.fixed,
                    has_video: profile.video_encoder_token.is_some(),
                    hidden,
                };
                // Hidden cards render as a one-line row; skip the snapshot probe.
                if hidden {
                    infos.push(base);
                    continue;
                }
                // ONVIF Profile S: only profiles bound to a video encoder
                // configuration support GetSnapshotUri. Metadata-only,
                // audio-only and analytics-only profiles will either return
                // a SOAP fault or hand back a URL the camera then 500s on
                // (seen on GeoVision LPR series). Skip them outright.
                if profile.video_encoder_token.is_none() {
                    debug!(
                        addr = %addr,
                        profile = %profile.token,
                        name = %profile.name,
                        "Skipping GetSnapshotUri (no video encoder configuration)"
                    );
                    infos.push(base);
                    continue;
                }
                match api::get_snapshot_uri(&addr, &creds, &profile.token).await {
                    Ok(snap) => {
                        let snapshot_url = api::resolve_snapshot_url(&addr, &snap.uri);
                        debug!(
                            addr = %addr,
                            profile = %profile.token,
                            name = %profile.name,
                            raw_uri = %snap.uri,
                            resolved_url = %snapshot_url,
                            invalid_after_connect = snap.invalid_after_connect,
                            "GetSnapshotUri OK"
                        );
                        infos.push(ProfileInfo {
                            snapshot_url: Some(snapshot_url),
                            invalid_after_connect: snap.invalid_after_connect,
                            ..base
                        });
                    }
                    Err(e) => {
                        warn!(
                            addr = %addr,
                            profile = %profile.token,
                            name = %profile.name,
                            error = %e,
                            "GetSnapshotUri FAILED — thumbnail will come from RTSP"
                        );
                        infos.push(base);
                    }
                }
            }
            // Tag the result with the addr it was fetched for so the
            // render layer can detect a stale fetch (different addr) and
            // show Loading instead of mixing one camera's snapshot URLs
            // with another camera's UI.
            Ok((addr.clone(), infos))
        }
    });

    // Read the currently-selected device addr at render time. Used both as
    // the ProfileCard key prefix (forces remount on device switch) and as a
    // freshness check against `profiles_res` — see comment below.
    let addr_now = ctx
        .selected
        .read()
        .and_then(|i| ctx.devices.read().get(i).map(|d| d.addr.clone()))
        .unwrap_or_default();
    let mut show_hidden = use_signal(|| false);

    rsx! {
        match &*profiles_res.read_unchecked() {
            None => rsx! {
                div { class: "thumb-loading", {i18n::t(locale, "loading")} }
            },
            Some(Err(_)) => rsx! {
                div { class: "thumb-empty", {i18n::t(locale, "no_profiles")} }
            },
            // Stale: result was fetched for the previous device, but the
            // user has already switched. Render Loading until use_resource
            // re-runs with the new addr — otherwise we'd build ProfileCards
            // with the old device's snapshot URLs and briefly display
            // its thumbnails on the new device's panel.
            Some(Ok((res_addr, _))) if res_addr != &addr_now => rsx! {
                div { class: "thumb-loading", {i18n::t(locale, "loading")} }
            },
            Some(Ok((_, infos))) if infos.is_empty() => rsx! {
                div { class: "thumb-empty", {i18n::t(locale, "no_profiles")} }
            },
            Some(Ok((_, infos))) => {
                let hidden_count = infos.iter().filter(|i| i.hidden).count();
                let showing = *show_hidden.read();
                let chevron = if showing { "chevron-down" } else { "chevron-right" };
                rsx! {
                    div { class: "thumb-grid",
                        for info in infos.iter().filter(|i| !i.hidden) {
                            ProfileCard {
                                key: "{addr_now}::{info.profile_token}",
                                gate,
                                device_addr: addr_now.clone(),
                                profile_token: info.profile_token.clone(),
                                profile_name: info.profile_name.clone(),
                                snapshot_url: info.snapshot_url.clone(),
                                invalid_after_connect: info.invalid_after_connect,
                                creds: info.creds.clone(),
                                fixed: info.fixed,
                                has_video: info.has_video,
                                on_changed: move |_| profiles_res.restart(),
                            }
                        }
                        NewProfileCard {
                            device_addr: addr_now.clone(),
                            on_created: move |_| profiles_res.restart(),
                        }
                    }
                    if hidden_count > 0 {
                        button {
                            class: "thumb-hidden-toggle",
                            onclick: move |_| { let v = !*show_hidden.peek(); show_hidden.set(v); },
                            Icon { name: chevron, size: 12 }
                            {i18n::t(locale, "profiles_hidden_count").replace("{n}", &hidden_count.to_string())}
                        }
                        if showing {
                            div { class: "thumb-hidden-list",
                                for info in infos.iter().filter(|i| i.hidden) {
                                    HiddenProfileRow {
                                        key: "{addr_now}::{info.profile_token}",
                                        device_addr: addr_now.clone(),
                                        profile_token: info.profile_token.clone(),
                                        profile_name: info.profile_name.clone(),
                                        on_changed: move |_| profiles_res.restart(),
                                    }
                                }
                            }
                        }
                    }
                }
            },
        }
    }
}

#[component]
fn HiddenProfileRow(
    device_addr: String,
    profile_token: String,
    profile_name: String,
    on_changed: EventHandler<()>,
) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    rsx! {
        div { class: "thumb-hidden-row",
            span { class: "thumb-label", "{profile_name}" }
            button {
                class: "thumb-action",
                title: i18n::t(locale, "profile_unhide"),
                onclick: move |_| {
                    crate::persist::set_profile_hidden(&device_addr, &profile_token, false);
                    on_changed.call(());
                },
                Icon { name: "eye", size: 12 }
            }
        }
    }
}

#[component]
fn ProfileCard(
    /// What the device said it can serve — drives which of the three
    /// per-profile jump buttons in the footer are offered.
    gate: api::DeviceGate,
    device_addr: String,
    profile_token: String,
    profile_name: String,
    snapshot_url: Option<String>,
    /// `true` when GetSnapshotUri reported `<InvalidAfterConnect>true</…>`.
    /// Card re-resolves a fresh URL each tick instead of caching the first
    /// one — see `ProfileInfo::invalid_after_connect`.
    invalid_after_connect: bool,
    creds: Credentials,
    fixed: bool,
    has_video: bool,
    /// Fired after a successful create/delete so the parent's
    /// `profiles_res` can restart and re-render the grid.
    on_changed: EventHandler<()>,
) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let mut view = ctx.view;
    let mut profile_sig = ctx.selected_profile;

    // Auto-refresh tick signal — triggers use_resource re-run every 3s
    let mut tick = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let next = tick.peek().wrapping_add(1);
            tick.set(next);
        }
    });

    // Sticky failure flag — once a snapshot URL has failed to fetch, stop
    // re-trying it on every tick. Hammering a known-broken URL every 3s
    // wastes bandwidth, spams logs, and on cameras with brute-force
    // protection (Hanwha/Samsung Wisenet) can lock the admin account.
    // The flag is reset implicitly when the user re-selects the device,
    // because that creates a fresh ProfileCard instance with a fresh signal.
    let mut broken = use_signal(|| false);

    // Stash extra clones up front for the delete-button onclick — the
    // use_resource closure below moves device_addr / creds / profile_token
    // into its async block, so these copies need to be split off first.
    let token_for_delete = profile_token.clone();
    let device_addr_for_delete = device_addr.clone();
    let creds_for_delete = creds.clone();
    let name_for_delete = profile_name.clone();

    // Fetch snapshot via Rust backend (handles Digest auth, self-signed certs).
    // For most cameras the cached URL is reused every tick. For cameras that
    // marked the URL single-use (`invalid_after_connect`), GetSnapshotUri is
    // called again per tick to mint a fresh URL — typically these embed a
    // per-call timestamp pointing at a temp file the camera then deletes.
    let token_for_fetch = profile_token.clone();
    let data_uri = use_resource(move || {
        let url = snapshot_url.clone();
        let creds = creds.clone();
        let device_addr = device_addr.clone();
        let profile_token = token_for_fetch.clone();
        let _tick = *tick.read(); // subscribe to tick signal for periodic refresh
        let already_broken = *broken.read();
        let needs_refresh = invalid_after_connect;
        async move {
            if already_broken {
                return Err("Snapshot endpoint marked broken — skipping retry".to_string());
            }

            // Resolve the URL — either reuse the cached one or re-fetch.
            // For invalid_after_connect cameras (LPR-style temp-file URLs
            // with per-call timestamps) we don't mark broken on errors;
            // many of those 500s are timing races where the camera serves
            // some frames but not others, so each tick re-tries.
            let url = if needs_refresh {
                match api::get_snapshot_uri(&device_addr, &creds, &profile_token).await {
                    Ok(snap) => api::resolve_snapshot_url(&device_addr, &snap.uri),
                    Err(e) => return Err(e),
                }
            } else {
                match url {
                    Some(u) => u,
                    // No ONVIF snapshot URI (Tapo): decode a key frame out
                    // of the RTSP stream instead. Not marked broken — the
                    // session may simply still be connecting.
                    None if has_video => {
                        return crate::video::rtsp::snapshot_jpeg(
                            &device_addr,
                            &profile_token,
                            &creds,
                        )
                        .await
                        .map(|b| crate::util::jpeg_data_uri(&b));
                    }
                    None => return Err("No snapshot".to_string()),
                }
            };

            let result = api::fetch_snapshot_data_uri(&url, &creds).await;
            if result.is_err() && !needs_refresh {
                broken.set(true);
            }
            result
        }
    });

    let selected = ctx
        .selected_profile
        .read()
        .as_ref()
        .map(|t| t == &profile_token)
        .unwrap_or(false);

    let card_class = if selected {
        "thumb-card thumb-card--selected"
    } else {
        "thumb-card"
    };

    let token_live = profile_token.clone();
    let token_img = profile_token.clone();
    let token_ptz = profile_token.clone();
    let name_for_save = profile_name.clone();
    let data_uri_for_save = data_uri;
    let delete_icon = if fixed { "eye-off" } else { "x" };

    rsx! {
        div {
            class: card_class,
            onclick: move |_| {
                profile_sig.set(Some(profile_token.clone()));
            },
            // Snapshot wrapper so the download button can be absolutely
            // positioned in the top-right of the image without joining
            // the page-switch action row in the footer.
            div { class: "thumb-img-wrap",
                match &*data_uri.read_unchecked() {
                    Some(Ok(src)) => rsx! {
                        img { class: "thumb-img", src: "{src}", alt: "{profile_name}" }
                    },
                    _ => rsx! {
                        div { class: "thumb-img thumb-img--placeholder",
                            Icon { name: "camera", size: 24 }
                        }
                    },
                }
                button {
                    class: "thumb-snapshot-save",
                    disabled: !matches!(&*data_uri_for_save.read_unchecked(), Some(Ok(_))),
                    title: if matches!(&*data_uri_for_save.read_unchecked(), Some(Ok(_))) { i18n::t(locale, "snapshot_save") } else { i18n::t(locale, "snapshot_save_no_image") },
                    onclick: move |e| {
                        e.stop_propagation();
                        let snap = match &*data_uri_for_save.read_unchecked() {
                            Some(Ok(uri)) => uri.clone(),
                            _ => return,
                        };
                        let name = name_for_save.clone();
                        let directory = crate::persist::snapshot_directory(ctx.snapshot_dir.read().as_deref());
                        let toast_ctx = ctx;
                        let saved_label = i18n::t(locale, "snapshot_saved").to_string();
                        let failed_label = i18n::t(locale, "snapshot_save_failed").to_string();
                        spawn(async move {
                            match crate::util::decode_jpeg_data_uri(&snap) {
                                Some(bytes) => match crate::util::save_snapshot(&directory, &name, &bytes) {
                                    Ok(path) => {
                                        tracing::info!(path = %path.display(), bytes = bytes.len(), "snapshot saved");
                                        toast_ctx.push_toast(
                                            crate::state::ToastLevel::Success,
                                            format!("{}: {}", saved_label, path.display()),
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, directory = %directory.display(), "snapshot save failed");
                                        toast_ctx.push_toast(
                                            crate::state::ToastLevel::Error,
                                            format!("{failed_label}: {e}"),
                                        );
                                    }
                                },
                                None => {
                                    toast_ctx.push_toast(
                                        crate::state::ToastLevel::Error,
                                        failed_label,
                                    );
                                }
                            }
                        });
                    },
                    Icon { name: "camera", size: 12 }
                }
                button {
                    class: "thumb-profile-delete",
                    title: if fixed { i18n::t(locale, "profile_hide") } else { i18n::t(locale, "profile_delete") },
                    onclick: move |e| {
                        e.stop_propagation();
                        let token = token_for_delete.clone();
                        let device_addr = device_addr_for_delete.clone();
                        let creds = creds_for_delete.clone();
                        let name = name_for_delete.clone();
                        let on_changed = on_changed;
                        // Fixed profiles can't be deleted camera-side; hide
                        // locally, no confirmation needed since it's reversible.
                        if fixed {
                            crate::persist::set_profile_hidden(&device_addr, &token, true);
                            ctx.push_toast(crate::state::ToastLevel::Success, i18n::t(locale, "profile_hidden"));
                            on_changed.call(());
                            return;
                        }
                        let confirm_label = i18n::t(locale, "btn_confirm").to_string();
                        let cancel_label = i18n::t(locale, "btn_cancel").to_string();
                        let title = i18n::t(locale, "profile_delete_title").to_string();
                        let message = i18n::t(locale, "profile_delete_confirm").replace("{name}", &name);
                        ctx.dialog.clone().set(Some(crate::state::ConfirmDialog {
                            title,
                            message,
                            confirm_label,
                            cancel_label,
                            dangerous: true,
                            on_confirm: Callback::new(move |_| {
                                let token = token.clone();
                                let device_addr = device_addr.clone();
                                let creds = creds.clone();
                                let on_changed = on_changed;
                                spawn(async move {
                                                            match api::delete_profile(&device_addr, &creds, &token).await {
                                        Ok(()) => {
                                            ctx.push_toast(crate::state::ToastLevel::Success,
                                                i18n::t(locale, "profile_deleted"));
                                            on_changed.call(());
                                        }
                                        Err(e) => ctx.push_toast(crate::state::ToastLevel::Error, e),
                                    }
                                });
                            }),
                        }));
                    },
                    Icon { name: delete_icon, size: 12 }
                }
            }
            div { class: "thumb-footer",
                span { class: "thumb-label", "{profile_name}" }
                div { class: "thumb-actions",
                    button {
                        class: "thumb-action",
                        title: i18n::t(locale, "nav_live_video"),
                        onclick: move |e| {
                            e.stop_propagation();
                            profile_sig.set(Some(token_live.clone()));
                            view.set(View::LiveVideo);
                        },
                        Icon { name: "video", size: 12 }
                    }
                    if gate.imaging {
                        button {
                            class: "thumb-action",
                            title: i18n::t(locale, "nav_imaging"),
                            onclick: move |e| {
                                e.stop_propagation();
                                profile_sig.set(Some(token_img.clone()));
                                view.set(View::ImagingSettings);
                            },
                            Icon { name: "sliders", size: 12 }
                        }
                    }
                    if gate.ptz {
                        button {
                            class: "thumb-action",
                            title: i18n::t(locale, "nav_ptz"),
                            onclick: move |e| {
                                e.stop_propagation();
                                profile_sig.set(Some(token_ptz.clone()));
                                view.set(View::PtzControl);
                            },
                            Icon { name: "crosshair", size: 12 }
                        }
                    }
                }
            }
        }
    }
}

/// "+ New profile" tile that toggles into an inline name input. Sits at
/// the end of the thumbnail grid so creating a profile feels like
/// adding another card. Inline form (vs popup modal) keeps the action
/// in context.
#[component]
fn NewProfileCard(device_addr: String, on_created: EventHandler<()>) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let mut editing = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // use_callback so the same dispatch can fire from the input's
    // Enter key handler and the Save button without lifetime gymnastics.
    let do_create = use_callback(move |_: ()| {
        let n = name.read().trim().to_string();
        if n.is_empty() || *saving.read() {
            return;
        }
        saving.set(true);
        let device_addr = device_addr.clone();
        let on_created = on_created;
        spawn(async move {
            // Profile creation needs admin creds. We use the global
            // credentials Ctx provides — manual devices override via
            // their own per-device creds in the same lookup path the
            // rest of the app uses.
            let creds = ctx
                .selected
                .peek()
                .and_then(|i| ctx.devices.peek().get(i).cloned())
                .map(|d| ctx.credentials_for(&d))
                .unwrap_or_else(|| ctx.global_credentials.peek().clone());
            match api::create_profile(&device_addr, &creds, &n).await {
                Ok(_) => {
                    ctx.push_toast(
                        crate::state::ToastLevel::Success,
                        i18n::t(locale, "profile_created"),
                    );
                    name.set(String::new());
                    editing.set(false);
                    on_created.call(());
                }
                Err(e) => ctx.push_toast(crate::state::ToastLevel::Error, e),
            }
            saving.set(false);
        });
    });

    rsx! {
        div { class: "thumb-card thumb-card--new",
            if *editing.read() {
                div { class: "new-profile-form",
                    input {
                        class: "form-input",
                        r#type: "text",
                        placeholder: i18n::t(locale, "profile_name_placeholder"),
                        value: "{name}",
                        oninput: move |evt: Event<FormData>| name.set(evt.value()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter {
                                do_create.call(());
                            } else if evt.key() == Key::Escape {
                                editing.set(false);
                                name.set(String::new());
                            }
                        },
                    }
                    div { class: "new-profile-form-actions",
                        button {
                            class: "btn btn-sm btn-ghost",
                            onclick: move |_| {
                                editing.set(false);
                                name.set(String::new());
                            },
                            {i18n::t(locale, "btn_cancel")}
                        }
                        button {
                            class: "btn btn-sm btn-primary",
                            disabled: name.read().trim().is_empty() || *saving.read(),
                            onclick: move |_| do_create.call(()),
                            {i18n::t(locale, "btn_save")}
                        }
                    }
                }
            } else {
                button {
                    class: "new-profile-btn",
                    onclick: move |_| editing.set(true),
                    Icon { name: "plus", size: 24 }
                    span { class: "new-profile-label",
                        {i18n::t(locale, "profile_create")}
                    }
                }
            }
        }
    }
}
