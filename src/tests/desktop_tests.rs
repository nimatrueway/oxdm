#[cfg(target_os = "linux")]
#[test]
fn hyprland_chrome_detects_session_signature_or_desktop_name() {
    for (signature, desktop, expected) in [
        (Some("instance-signature"), None, true),
        (Some("instance-signature"), Some("GNOME"), true),
        (None, Some("Hyprland"), true),
        (None, Some("Hyprland:wlroots"), true),
        (None, Some("wlroots:hyprland"), true),
        (Some(""), Some("Hyprland"), true),
        (Some(""), None, false),
        (Some(" "), Some("GNOME"), false),
        (None, Some(""), false),
        (None, Some("GNOME"), false),
        (None, Some("sway"), false),
        (None, Some("NotHyprland"), false),
        (None, None, false),
    ] {
        assert_eq!(
            crate::hyprland_session(signature, desktop),
            expected,
            "signature={signature:?}, desktop={desktop:?}"
        );
    }
}

#[test]
fn hyprland_chrome_removes_only_the_window_decorations() {
    let standard = crate::main_window_builder(false).window;
    let hyprland = crate::main_window_builder(true).window;

    assert!(standard.decorations);
    assert!(!hyprland.decorations);
    assert_eq!(hyprland.title, standard.title);
    assert_eq!(hyprland.inner_size, standard.inner_size);
    assert_eq!(
        hyprland.inner_size_constraints,
        standard.inner_size_constraints
    );
    assert_eq!(hyprland.resizable, standard.resizable);
    assert_eq!(hyprland.always_on_top, standard.always_on_top);
    assert!(hyprland.window_icon.is_some());
}
