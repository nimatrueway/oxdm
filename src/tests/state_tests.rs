use crate::persist::{settings_tab_from_str, settings_tab_to_str, view_from_str, view_to_str};
use crate::state::{Credentials, Locale, SettingsTab, Theme, View};

/// session.toml stores these as strings; a variant that doesn't survive the
/// round trip silently reopens on Welcome / Identification.
#[test]
fn session_view_and_tab_strings_round_trip() {
    for v in [
        View::Welcome,
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
fn theme_cycles_correctly() {
    assert_eq!(Theme::Dark.next(), Theme::Light);
    assert_eq!(Theme::Light.next(), Theme::Classic);
    assert_eq!(Theme::Classic.next(), Theme::Dark);
}

#[test]
fn theme_css_class_contains_theme_name() {
    assert!(Theme::Dark.css_class().contains("dark"));
    assert!(Theme::Light.css_class().contains("light"));
    assert!(Theme::Classic.css_class().contains("classic"));
}

#[test]
fn locale_cycles_correctly() {
    assert_eq!(Locale::En.next(), Locale::ZhTw);
    assert_eq!(Locale::ZhTw.next(), Locale::Ru);
    assert_eq!(Locale::Ru.next(), Locale::En);
}

#[test]
fn locale_labels_are_short() {
    for locale in [Locale::En, Locale::ZhTw, Locale::Ru] {
        let label = locale.label();
        assert!(label.len() <= 4, "label too long: {label}");
    }
}

#[test]
fn credentials_default_is_empty() {
    let c = Credentials::default();
    assert!(c.username.is_empty());
    assert!(c.password.is_empty());
}
