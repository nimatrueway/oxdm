#![allow(non_snake_case)]
use crate::components::{AboutDialog, DefaultCredentialsForm, Icon, LogViewer};
use crate::i18n;
use crate::state::{Ctx, Locale, Theme, View};
use dioxus::prelude::*;

#[component]
pub fn AppSettingsView() -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let theme = *ctx.theme.read();
    let mut about_open = use_signal(|| false);
    let mut logs_open = use_signal(|| false);
    let log_path = crate::log_dir().map(|path| path.display().to_string());
    let language = match locale {
        Locale::En => "en",
        Locale::ZhTw => "zh_tw",
        Locale::Ru => "ru",
    };
    let folders = [
        (
            ctx.snapshot_dir,
            crate::persist::snapshot_directory(ctx.snapshot_dir.read().as_deref()),
            "app_settings_snapshot_folder",
        ),
        (
            ctx.recording_dir,
            crate::persist::recording_directory(ctx.recording_dir.read().as_deref()),
            "app_settings_recording_folder",
        ),
    ];

    rsx! {
        div { class: "app-settings-view",
            header { class: "content-header app-settings-header",
                button {
                    class: "icon-btn",
                    title: i18n::t(locale, "app_settings_back"),
                    aria_label: i18n::t(locale, "app_settings_back"),
                    onclick: move |_| ctx.view.clone().set(if ctx.selected.peek().is_some() { View::LiveVideo } else { View::Welcome }),
                    Icon { name: "arrow-left", size: 16 }
                }
                h1 { class: "content-title", {i18n::t(locale, "app_settings_title")} }
            }
            div { class: "app-settings-body",
                section { class: "app-settings-section",
                    h2 { {i18n::t(locale, "app_settings_appearance")} }
                    div { class: "app-settings-row",
                        span { {i18n::t(locale, "tooltip_theme")} }
                        div { class: "app-settings-modes", role: "group", aria_label: i18n::t(locale, "tooltip_theme"),
                            for (choice, icon, label) in [
                                (Theme::Dark, "moon", "app_settings_theme_dark"),
                                (Theme::Light, "sun", "app_settings_theme_light"),
                                (Theme::Classic, "monitor", "app_settings_theme_classic"),
                            ] {
                                button {
                                    class: if theme == choice { "app-settings-mode app-settings-mode--active" } else { "app-settings-mode" },
                                    aria_pressed: theme == choice,
                                    onclick: move |_| ctx.theme.clone().set(choice),
                                    Icon { name: icon, size: 14 }
                                    {i18n::t(locale, label)}
                                }
                            }
                        }
                    }
                    label { class: "app-settings-row",
                        span { {i18n::t(locale, "tooltip_language")} }
                        select {
                            class: "app-settings-select",
                            value: language,
                            onchange: move |event| ctx.locale.clone().set(crate::persist::locale_from_str(&event.value())),
                            option { value: "en", {i18n::t(locale, "app_settings_language_en")} }
                            option { value: "zh_tw", {i18n::t(locale, "app_settings_language_zh_tw")} }
                            option { value: "ru", {i18n::t(locale, "app_settings_language_ru")} }
                        }
                    }
                    label { class: "app-settings-row",
                        span { {i18n::t(locale, "app_settings_camera_size")} }
                        input {
                            class: "app-settings-size",
                            r#type: "range",
                            min: "48", max: "112", step: "16",
                            value: "{ctx.camera_item_size}",
                            oninput: move |event| {
                                if let Ok(size) = event.value().parse::<u16>() {
                                    ctx.camera_item_size.clone().set(size.clamp(48, 112));
                                }
                            },
                        }
                        output { "{ctx.camera_item_size}px" }
                    }
                }
                section { class: "app-settings-section",
                    h2 { {i18n::t(locale, "app_settings_storage")} }
                    for (mut directory, path, label) in folders {
                        div { class: "app-settings-folder",
                            span { {i18n::t(locale, label)} }
                            code { "{path.display()}" }
                            button {
                                class: "icon-btn",
                                title: i18n::t(locale, "app_settings_choose_folder"),
                                aria_label: format!("{}: {}", i18n::t(locale, "app_settings_choose_folder"), i18n::t(locale, label)),
                                onclick: move |_| {
                                    let path = path.clone();
                                    spawn(async move {
                                        if let Some(folder) = rfd::AsyncFileDialog::new()
                                            .set_title(i18n::t(locale, label))
                                            .set_directory(path)
                                            .pick_folder()
                                            .await
                                        {
                                            directory.set(Some(folder.path().to_path_buf()));
                                        }
                                    });
                                },
                                Icon { name: "folder", size: 16 }
                            }
                            button {
                                class: "icon-btn",
                                title: i18n::t(locale, "app_settings_default_folder"),
                                aria_label: format!("{}: {}", i18n::t(locale, "app_settings_default_folder"), i18n::t(locale, label)),
                                disabled: directory.read().is_none(),
                                onclick: move |_| directory.set(None),
                                Icon { name: "x", size: 14 }
                            }
                        }
                    }
                }
                section { class: "app-settings-section",
                    h2 { {i18n::t(locale, "cred_global_title")} }
                    DefaultCredentialsForm {}
                    label { class: "app-settings-check", title: i18n::t(locale, "about_tls_strict_hint"),
                        input {
                            r#type: "checkbox",
                            checked: *ctx.tls_strict.read(),
                            onchange: move |event: Event<FormData>| ctx.tls_strict.clone().set(event.checked()),
                        }
                        span { {i18n::t(locale, "about_tls_strict")} }
                    }
                }
                section { class: "app-settings-section",
                    h2 { {i18n::t(locale, "app_settings_logging")} }
                    label { class: "app-settings-check",
                        input {
                            r#type: "checkbox",
                            checked: *ctx.log_to_file.read(),
                            onchange: move |event: Event<FormData>| ctx.log_to_file.clone().set(event.checked()),
                        }
                        span { {i18n::t(locale, "about_log_to_file")} }
                    }
                    p { class: "app-settings-note", {i18n::t(locale, "about_log_takes_effect")} }
                    if let Some(path) = log_path {
                        div { class: "app-settings-path",
                            code { "{path}" }
                            button {
                                class: "icon-btn",
                                title: i18n::t(locale, "about_open_logs"),
                                aria_label: i18n::t(locale, "about_open_logs"),
                                onclick: move |_| {
                                    if let Some(path) = crate::log_dir() {
                                        if let Err(error) = opener::open(path) {
                                            tracing::warn!(%error, "open log directory failed");
                                        }
                                    }
                                },
                                Icon { name: "folder", size: 16 }
                            }
                        }
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        onclick: move |_| logs_open.set(true),
                        Icon { name: "file-text", size: 14 }
                        {i18n::t(locale, "logs_title")}
                    }
                }
                div { class: "app-settings-actions",
                    button {
                        class: "btn btn-sm btn-ghost",
                        onclick: move |_| about_open.set(true),
                        Icon { name: "help-circle", size: 14 }
                        {i18n::t(locale, "app_settings_about")}
                    }
                }
            }
        }
        AboutDialog { open: about_open }
        LogViewer { open: logs_open }
    }
}
