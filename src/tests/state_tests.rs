use crate::persist::{
    locale_from_str, settings_tab_from_str, settings_tab_to_str, theme_from_str, view_from_str,
    view_to_str,
};
use crate::state::{Credentials, Locale, SettingsTab, Theme, View, WorkspaceTab};

#[test]
fn workspace_tabs_group_existing_views_without_changing_saved_routes() {
    for view in [View::LiveVideo, View::ImagingSettings, View::PtzControl] {
        assert_eq!(
            view.workspace_tab(SettingsTab::Identification),
            Some(WorkspaceTab::Live)
        );
    }
    for view in [View::DeviceSettings, View::Osd, View::IoControl] {
        assert_eq!(
            view.workspace_tab(SettingsTab::Identification),
            Some(WorkspaceTab::Settings)
        );
    }
    for tab in [SettingsTab::Health, SettingsTab::Quirks] {
        assert_eq!(
            View::DeviceSettings.workspace_tab(tab),
            Some(WorkspaceTab::Settings)
        );
    }
    assert_eq!(
        View::Events.workspace_tab(SettingsTab::Identification),
        Some(WorkspaceTab::Settings)
    );
    assert_eq!(
        View::Recordings.workspace_tab(SettingsTab::Identification),
        Some(WorkspaceTab::Recordings)
    );
    for view in [View::Welcome, View::AppSettings, View::HealthOverview] {
        assert_eq!(view.workspace_tab(SettingsTab::Health), None);
    }
}

/// session.toml stores these as strings; a variant that doesn't survive the
/// round trip silently reopens on Welcome / Identification.
#[test]
fn session_view_and_tab_strings_round_trip() {
    for v in [
        View::Welcome,
        View::AppSettings,
        View::DeviceSettings,
        View::LiveVideo,
        View::ImagingSettings,
        View::PtzControl,
        View::Events,
        View::Osd,
        View::IoControl,
        View::Recordings,
        View::HealthOverview,
    ] {
        assert_eq!(view_from_str(view_to_str(v)), v);
    }
    for t in [
        SettingsTab::Identification,
        SettingsTab::Imaging,
        SettingsTab::Profiles,
        SettingsTab::Network,
        SettingsTab::Time,
        SettingsTab::Users,
        SettingsTab::Maintenance,
        SettingsTab::Health,
        SettingsTab::Quirks,
    ] {
        assert_eq!(settings_tab_from_str(settings_tab_to_str(t)), t);
    }
    assert_eq!(view_from_str("garbage"), View::Welcome);
    assert_eq!(settings_tab_from_str(""), SettingsTab::Identification);
}

#[test]
fn theme_css_class_contains_theme_name() {
    assert!(Theme::Dark.css_class().contains("dark"));
    assert!(Theme::Light.css_class().contains("light"));
    assert!(Theme::Classic.css_class().contains("classic"));
}

#[test]
fn app_settings_choices_match_persisted_preferences() {
    for (value, expected) in [
        ("dark", Theme::Dark),
        ("light", Theme::Light),
        ("classic", Theme::Classic),
    ] {
        assert_eq!(theme_from_str(value), expected);
    }
    for (value, expected) in [
        ("en", Locale::En),
        ("zh_tw", Locale::ZhTw),
        ("ru", Locale::Ru),
    ] {
        assert_eq!(locale_from_str(value), expected);
    }
}

#[test]
fn credentials_default_is_empty() {
    let c = Credentials::default();
    assert!(c.username.is_empty());
    assert!(c.password.is_empty());
}

#[test]
fn sidebar_width_keeps_both_panels_usable() {
    assert_eq!(crate::state::clamp_sidebar_width(50.0, 1280.0), 200.0);
    assert_eq!(crate::state::clamp_sidebar_width(320.0, 1280.0), 320.0);
    assert_eq!(crate::state::clamp_sidebar_width(900.0, 1280.0), 560.0);
    assert_eq!(crate::state::clamp_sidebar_width(560.0, 900.0), 495.0);
}

#[test]
fn camera_item_size_defaults_clamps_and_survives_config_round_trip() {
    let mut config = crate::persist::ConfigFile::default();
    assert_eq!(config.camera_item_size(), 64);
    config.camera_item_size = Some(96);
    let encoded = toml::to_string(&config).unwrap();
    let restored: crate::persist::ConfigFile = toml::from_str(&encoded).unwrap();
    assert_eq!(restored.camera_item_size(), 96);
    config.camera_item_size = Some(0);
    assert_eq!(config.camera_item_size(), 48);
    config.camera_item_size = Some(u16::MAX);
    assert_eq!(config.camera_item_size(), 112);
}

#[test]
fn ptz_speed_defaults_clamps_and_survives_config_round_trip() {
    let mut config: crate::persist::ConfigFile = toml::from_str("theme = 'light'").unwrap();
    assert_eq!(config.ptz_speed(), 0.5);
    config.ptz_speed = Some(0.65);
    let encoded = toml::to_string(&config).unwrap();
    let restored: crate::persist::ConfigFile = toml::from_str(&encoded).unwrap();
    assert_eq!(restored.ptz_speed(), 0.65);
    for (value, expected) in [
        (-1.0, 0.1),
        (0.0, 0.1),
        (2.0, 1.0),
        (f32::NAN, 0.5),
        (f32::INFINITY, 0.5),
        (f32::NEG_INFINITY, 0.5),
    ] {
        config.ptz_speed = Some(value);
        assert_eq!(config.ptz_speed(), expected);
    }
}

#[test]
fn sidebar_collapse_defaults_to_expanded() {
    assert!(!crate::persist::ConfigFile::default().sidebar_collapsed);
    for encoded in ["", "theme = 'light'"] {
        let config: crate::persist::ConfigFile = toml::from_str(encoded).unwrap();
        assert!(!config.sidebar_collapsed);
    }
}

#[test]
fn saved_preferences_round_trip_without_credentials() {
    for sidebar_collapsed in [true, false] {
        let config = crate::persist::ConfigOut {
            theme: "light".to_string(),
            locale: "ru".to_string(),
            log_to_file: true,
            tls_strict: true,
            snapshot_dir: None,
            recording_dir: None,
            camera_item_size: 96,
            sidebar_collapsed,
            ptz_speed: 0.65,
        };
        let encoded = toml::to_string_pretty(&config).unwrap();
        let saved: toml::Table = toml::from_str(&encoded).unwrap();
        assert!(!saved.contains_key("username"));
        assert!(!saved.contains_key("password"));
        assert!(!saved.contains_key("snapshot_dir"));
        assert!(!saved.contains_key("recording_dir"));
        let restored: crate::persist::ConfigFile = toml::from_str(&encoded).unwrap();
        assert_eq!(restored.sidebar_collapsed, sidebar_collapsed);
        assert_eq!(restored.ptz_speed(), 0.65);
        assert_eq!(restored.theme, "light");
        assert_eq!(restored.locale, "ru");
        assert!(restored.log_to_file);
        assert!(restored.tls_strict);
        assert_eq!(restored.camera_item_size(), 96);
    }
}

#[test]
fn capture_directories_accept_legacy_config_and_round_trip_custom_paths() {
    let mut config: crate::persist::ConfigFile = toml::from_str("theme = 'light'").unwrap();
    assert!(config.snapshot_dir.is_none());
    assert!(config.recording_dir.is_none());
    assert_eq!(
        crate::persist::snapshot_directory(None).file_name(),
        Some(std::ffi::OsStr::new("OxDM")),
    );
    assert_eq!(
        crate::persist::recording_directory(None).file_name(),
        Some(std::ffi::OsStr::new("OxDM")),
    );

    let snapshots = std::env::temp_dir().join("custom snapshots");
    let recordings = std::env::temp_dir().join("custom recordings");
    config.snapshot_dir = Some(snapshots.clone());
    config.recording_dir = Some(recordings.clone());
    let encoded = toml::to_string(&config).unwrap();
    let restored: crate::persist::ConfigFile = toml::from_str(&encoded).unwrap();
    assert_eq!(
        crate::persist::snapshot_directory(restored.snapshot_dir.as_deref()),
        snapshots,
    );
    assert_eq!(
        crate::persist::recording_directory(restored.recording_dir.as_deref()),
        recordings,
    );
}
