#![allow(non_snake_case)]
use crate::components::{AboutDialog, DefaultCredentialsForm, Icon, LogViewer};
use crate::state::{ConfirmDialog, Ctx, Locale, Theme, ToastLevel, View};
use crate::{i18n, persist};
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
                SettingsBackupSection {}
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

#[component]
fn SettingsBackupSection() -> Element {
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();
    let mut include_credentials = use_signal(|| false);
    let mut busy = use_signal(|| false);

    rsx! {
        section { class: "app-settings-section",
            h2 { {i18n::t(locale, "backup_title")} }
            label { class: "app-settings-check",
                input {
                    r#type: "checkbox",
                    checked: include_credentials(),
                    disabled: busy(),
                    onchange: move |event| include_credentials.set(event.checked()),
                }
                span { {i18n::t(locale, "backup_include_credentials")} }
            }
            if include_credentials() {
                p { class: "form-hint", {i18n::t(locale, "backup_credentials_warning")} }
            }
            div { class: "app-settings-actions",
                button {
                    class: "btn btn-sm btn-ghost",
                    disabled: busy() || !*ctx.loaded.read(),
                    onclick: move |_| {
                        let include = include_credentials();
                        let backup = persist::SettingsBackup::capture(
                            persist::ConfigOut {
                                theme: persist::theme_to_str(*ctx.theme.peek()).into(),
                                locale: persist::locale_to_str(*ctx.locale.peek()).into(),
                                log_to_file: *ctx.log_to_file.peek(),
                                tls_strict: *ctx.tls_strict.peek(),
                                snapshot_dir: ctx.snapshot_dir.peek().clone(),
                                recording_dir: ctx.recording_dir.peek().clone(),
                                camera_item_size: *ctx.camera_item_size.peek(),
                                sidebar_collapsed: *ctx.sidebar_collapsed.peek(),
                                ptz_speed: *ctx.ptz_speed.peek(),
                            },
                            &ctx.global_credentials.peek(),
                            &ctx.devices.peek(),
                            &ctx.health_groups.peek(),
                            persist::load_hidden_profiles().into_iter().collect(),
                            include,
                        );
                        busy.set(true);
                        spawn(async move {
                            if let Some(file) = rfd::AsyncFileDialog::new()
                                .set_title(i18n::t(locale, "backup_export"))
                                .set_file_name("oxdm-settings.json")
                                .add_filter(i18n::t(locale, "backup_file_type"), &["json"])
                                .save_file().await
                            {
                                let result: Result<(), &str> = async {
                                    let bytes = tokio::task::spawn_blocking(move || {
                                        persist::encode_settings_backup(&backup)
                                    }).await.map_err(|_| "backup_invalid")??;
                                    tokio::fs::write(file.path(), bytes).await.map_err(|_| "backup_file_error")
                                }.await;
                                match result {
                                    Ok(()) => ctx.push_toast(ToastLevel::Success, i18n::t(locale, "backup_exported")),
                                    Err(key) => ctx.push_toast(ToastLevel::Error, i18n::t(locale, key)),
                                }
                            }
                            busy.set(false);
                        });
                    },
                    Icon { name: "download", size: 14 }
                    {i18n::t(locale, "backup_export")}
                }
                button {
                    class: "btn btn-sm btn-ghost",
                    disabled: busy() || !*ctx.loaded.read(),
                    onclick: move |_| {
                        busy.set(true);
                        spawn(async move {
                            if let Some(file) = rfd::AsyncFileDialog::new()
                                .set_title(i18n::t(locale, "backup_import"))
                                .add_filter(i18n::t(locale, "backup_file_type"), &["json"])
                                .pick_file().await
                            {
                                let result = tokio::task::spawn_blocking(move || persist::read_settings_backup(file.path()))
                                    .await.unwrap_or(Err("backup_file_error"));
                                match result {
                                    Ok(bytes) => match persist::decode_settings_backup(&bytes) {
                                        Ok(backup) => confirm_backup_import(ctx, backup, busy),
                                        Err(key) => ctx.push_toast(ToastLevel::Error, i18n::t(locale, key)),
                                    },
                                    Err(key) => ctx.push_toast(ToastLevel::Error, i18n::t(locale, key)),
                                }
                            }
                            busy.set(false);
                        });
                    },
                    Icon { name: "arrow-up", size: 14 }
                    {i18n::t(locale, "backup_import")}
                }
                if busy() {
                    span { role: "status", {i18n::t(locale, "loading")} }
                }
            }
        }
    }
}

fn confirm_backup_import(mut ctx: Ctx, backup: persist::SettingsBackup, mut busy: Signal<bool>) {
    let locale = *ctx.locale.peek();
    ctx.dialog.set(Some(ConfirmDialog {
        title: i18n::t(locale, "backup_confirm_title").into(),
        message: i18n::t(locale, "backup_confirm_message").into(),
        confirm_label: i18n::t(locale, "backup_import").into(),
        cancel_label: i18n::t(locale, "btn_cancel").into(),
        dangerous: true,
        on_confirm: EventHandler::new(move |_| {
            let backup = backup.clone();
            busy.set(true);
            dioxus::dioxus_core::spawn_forever(async move {
                apply_settings_backup(ctx, backup).await;
                if let Ok(mut value) = busy.try_write() {
                    *value = false;
                }
            });
        }),
    }));
}

async fn apply_settings_backup(mut ctx: Ctx, backup: persist::SettingsBackup) {
    let locale = *ctx.locale.peek();
    if backup.includes_credentials() {
        let mut global = ctx.global_credentials.peek().clone();
        let mut devices = ctx.devices.peek().clone();
        let mut groups = ctx.health_groups.peek().clone();
        backup.merge_into(&mut global, &mut devices, &mut groups);
        let result = tokio::task::spawn_blocking(move || {
            persist::save_backup_credentials(&global, &devices, &groups)
        })
        .await
        .unwrap_or(Err("backup_keychain_error"));
        if let Err(key) = result {
            ctx.push_toast(ToastLevel::Error, i18n::t(locale, key));
            return;
        }
    }
    let mut global = ctx.global_credentials.peek().clone();
    let mut devices = ctx.devices.peek().clone();
    let mut groups = ctx.health_groups.peek().clone();
    backup.merge_into(&mut global, &mut devices, &mut groups);
    let preferences = &backup.preferences;
    ctx.theme.set(persist::theme_from_str(&preferences.theme));
    ctx.locale
        .set(persist::locale_from_str(&preferences.locale));
    ctx.log_to_file.set(preferences.log_to_file);
    ctx.tls_strict.set(preferences.tls_strict);
    ctx.snapshot_dir.set(preferences.snapshot_dir.clone());
    ctx.recording_dir.set(preferences.recording_dir.clone());
    ctx.camera_item_size
        .set(preferences.camera_item_size.clamp(48, 112));
    ctx.sidebar_collapsed.set(preferences.sidebar_collapsed);
    ctx.ptz_speed.set(preferences.ptz_speed.clamp(0.1, 1.0));
    ctx.global_credentials.set(global);
    ctx.devices.set(devices);
    ctx.health_groups.set(groups);
    persist::merge_hidden_profiles(&backup.hidden_profiles);
    crate::sessions::invalidate_all();
    crate::device_ops::reverify_auth(ctx, ctx.devices);
    ctx.push_toast(
        ToastLevel::Success,
        i18n::t(*ctx.locale.peek(), "backup_imported"),
    );
}
