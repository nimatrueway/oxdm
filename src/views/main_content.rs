#![allow(non_snake_case)]
use crate::api;
use crate::components::{Icon, LensBrand, ProfileSelector, ProfileThumbnails};
use crate::i18n;
use crate::state::{Credentials, Ctx, SettingsTab, View};
use crate::views::app_settings::AppSettingsView;
use crate::views::events::EventsView;
use crate::views::health_overview::HealthOverviewView;
use crate::views::imaging::ImagingView;
use crate::views::io_control::IoControlView;
use crate::views::live_video::LiveVideoView;
use crate::views::osd::OsdView;
use crate::views::recordings::RecordingsView;
use crate::views::settings::{
    HealthTab, IdentificationTab, MaintenanceTab, NetworkTab, QuirkTab, TimeTab, UsersTab,
};
use dioxus::prelude::*;

#[component]
pub fn MainContent() -> Element {
    let ctx = use_context::<Ctx>();
    let view = *ctx.view.read();

    // Derive addr and effective credentials as reactive memos
    let addr = use_memo(move || {
        let devices = ctx.devices.read();
        let selected = *ctx.selected.read();
        selected
            .and_then(|i| devices.get(i))
            .map(|d| d.addr.clone())
            .unwrap_or_default()
    });

    let creds = use_memo(move || {
        let devices = ctx.devices.read();
        let selected = *ctx.selected.read();
        selected
            .and_then(|i| devices.get(i))
            .map(|d| ctx.credentials_for(d))
            .unwrap_or_else(|| ctx.global_credentials.read().clone())
    });

    // Use addr as a render key so views with internal `use_resource`
    // (LiveVideoView, ImagingView, PtzControlView) remount cleanly on
    // device switch — otherwise their previous-device fetch result stays
    // visible until the new fetch lands, which feels like a stale UI bug.
    let addr_key = addr.read().clone();

    let gate_res = use_resource(move || {
        let addr = addr.read().clone();
        let creds = creds.read().clone();
        async move {
            let gate = if addr.is_empty() {
                api::DeviceGate::permissive()
            } else {
                api::device_gate(&addr, &creds).await
            };
            (addr, gate)
        }
    });
    let gate = match &*gate_res.read_unchecked() {
        Some((res_addr, gate)) if res_addr == &addr_key => *gate,
        _ => api::DeviceGate::permissive(),
    };

    rsx! {
        section {
            class: "workspace",
            onkeydown: move |event| {
                if matches!(event.key(), Key::ArrowUp | Key::ArrowDown) {
                    event.stop_propagation();
                }
            },
            main { class: "main-content",
                match view {
                    View::Welcome         => rsx! { WelcomeView {} },
                    View::AppSettings     => rsx! { AppSettingsView {} },
                    View::HealthOverview  => rsx! { HealthOverviewView {} },
                    View::DeviceSettings | View::Events | View::Osd | View::IoControl => rsx! {
                        DeviceSettingsView { key: "{addr_key}", addr, creds, gate }
                    },
                    View::LiveVideo | View::ImagingSettings | View::PtzControl => rsx! {
                        LiveVideoView { key: "{addr_key}", addr, creds, gate }
                    },
                    View::Recordings      => rsx! { RecordingsView { key: "{addr_key}", addr, creds } },
                }
            }
        }
    }
}

#[component]
fn WelcomeView() -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();

    rsx! {
        div { class: "welcome",
            div { class: "welcome-icon",
                LensBrand { size: 128 }
            }
            h1  { class: "welcome-title", {i18n::t(locale, "app_name")} }
            p   { class: "welcome-sub",   {i18n::t(locale, "app_subtitle")} }
            p   { class: "welcome-hint",  {i18n::t(locale, "welcome_hint")} }
        }
    }
}

// ── Device Settings (tabbed view) ───────────────────────────────────────────

const SETTINGS_TABS: &[(SettingsTab, &str)] = &[
    (SettingsTab::Identification, "tab_identification"),
    (SettingsTab::Imaging, "workspace_image"),
    (SettingsTab::Profiles, "workspace_profiles"),
    (SettingsTab::Network, "tab_network"),
    (SettingsTab::Time, "tab_time"),
    (SettingsTab::Users, "tab_users"),
    (SettingsTab::Maintenance, "tab_maintenance"),
];

#[component]
fn DeviceSettingsView(
    addr: Memo<String>,
    creds: Memo<Credentials>,
    gate: api::DeviceGate,
) -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let active = *ctx.settings_tab.read();
    let view = *ctx.view.read();
    let diagnostics = view == View::Events
        || (view == View::DeviceSettings
            && matches!(active, SettingsTab::Health | SettingsTab::Quirks));
    let profile_key = ctx.selected_profile.read().clone().unwrap_or_default();

    rsx! {
        div { class: "settings-view",
            div { class: "settings-navigation",
                button {
                    class: "icon-btn settings-back",
                    title: i18n::t(locale, "workspace_back_video"),
                    aria_label: i18n::t(locale, "workspace_back_video"),
                    onclick: move |_| ctx.view.clone().set(View::LiveVideo),
                    Icon { name: "arrow-left", size: 16 }
                }
                nav { class: "tab-bar settings-tabs", aria_label: i18n::t(locale, "nav_settings"),
                    for &(tab, label) in SETTINGS_TABS {
                        if tab != SettingsTab::Imaging || gate.imaging || active == tab {
                            button {
                                class: if view == View::DeviceSettings && active == tab { "tab tab--active" } else { "tab" },
                                aria_current: if view == View::DeviceSettings && active == tab { "page" } else { "false" },
                                onclick: move |_| {
                                    ctx.settings_tab.clone().set(tab);
                                    ctx.view.clone().set(View::DeviceSettings);
                                },
                                {i18n::t(locale, label)}
                            }
                        }
                    }
                    for (target, available, label) in [
                        (View::Osd, gate.osd, "nav_osd"),
                        (View::IoControl, gate.io, "nav_io_control"),
                    ] {
                        if available || view == target {
                            button {
                                class: if view == target { "tab tab--active" } else { "tab" },
                                aria_current: if view == target { "page" } else { "false" },
                                onclick: move |_| ctx.view.clone().set(target),
                                {i18n::t(locale, label)}
                            }
                        }
                    }
                    button {
                        class: if diagnostics { "tab tab--active" } else { "tab" },
                        aria_current: if diagnostics { "page" } else { "false" },
                        onclick: move |_| {
                            if !diagnostics {
                                ctx.settings_tab.clone().set(SettingsTab::Health);
                                ctx.view.clone().set(View::DeviceSettings);
                            }
                        },
                        Icon { name: "activity", size: 14 }
                        {i18n::t(locale, "workspace_diagnostics")}
                    }
                }
            }
            if diagnostics {
                nav { class: "tab-bar diagnostics-tabs", aria_label: i18n::t(locale, "workspace_diagnostics"),
                    for (target, tab, icon, label) in [
                        (View::DeviceSettings, SettingsTab::Health, "activity", "tab_health"),
                        (View::Events, SettingsTab::Health, "bell", "nav_events"),
                        (View::DeviceSettings, SettingsTab::Quirks, "git-compare", "tab_quirks"),
                    ] {
                        if target != View::Events || gate.events || view == View::Events {
                            button {
                                class: if view == target && (target == View::Events || active == tab) { "tab tab--active" } else { "tab" },
                                onclick: move |_| {
                                    ctx.settings_tab.clone().set(tab);
                                    ctx.view.clone().set(target);
                                },
                                Icon { name: icon, size: 14 }
                                {i18n::t(locale, label)}
                            }
                        }
                    }
                }
            }

            div { class: "tab-content",
                if view == View::Osd || (view == View::DeviceSettings && active == SettingsTab::Imaging) {
                    ProfileSelector { addr, creds }
                }
                match view {
                    View::Events => rsx! { EventsView { addr, creds } },
                    View::Osd => rsx! { OsdView { key: "{profile_key}", addr, creds } },
                    View::IoControl => rsx! { IoControlView { addr, creds } },
                    _ => match active {
                        SettingsTab::Identification => rsx! { IdentificationTab { addr, creds } },
                        SettingsTab::Imaging        => rsx! { ImagingView { key: "{profile_key}", addr, creds } },
                        SettingsTab::Profiles       => rsx! { div { class: "profile-manager", ProfileThumbnails { gate } } },
                        SettingsTab::Network        => rsx! { NetworkTab { addr, creds } },
                        SettingsTab::Time           => rsx! { TimeTab { addr, creds } },
                        SettingsTab::Users          => rsx! { UsersTab { addr, creds } },
                        SettingsTab::Maintenance    => rsx! { MaintenanceTab { addr, creds } },
                        SettingsTab::Health         => rsx! { HealthTab { addr, creds } },
                        SettingsTab::Quirks         => rsx! { QuirkTab { addr, creds } },
                    },
                }
            }
        }
    }
}
