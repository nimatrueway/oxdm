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

#[test]
fn theater_window_allows_small_video_sizes_without_changing_normal_minimum() {
    assert_eq!(
        crate::main_window_min_size(true),
        dioxus::desktop::LogicalSize::new(320.0, 180.0)
    );
    assert_eq!(
        crate::main_window_min_size(false),
        dioxus::desktop::LogicalSize::new(900.0, 500.0)
    );
}

#[test]
fn leaving_theater_restores_only_dimensions_below_the_layout_minimum() {
    for (width, height, expected_width, expected_height) in [
        (320.0, 180.0, 900.0, 500.0),
        (640.0, 600.0, 900.0, 600.0),
        (1200.0, 300.0, 1200.0, 500.0),
        (1280.0, 800.0, 1280.0, 800.0),
    ] {
        assert_eq!(
            crate::main_window_layout_size(dioxus::desktop::LogicalSize::new(width, height)),
            dioxus::desktop::LogicalSize::new(expected_width, expected_height)
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a display and window manager honoring native always-on-top requests"]
fn theater_window_updates_native_stacking_on_enter_and_exit() {
    use dioxus::desktop::tao::{
        event_loop::{ControlFlow, EventLoopBuilder},
        platform::{run_return::EventLoopExtRunReturn, unix::EventLoopBuilderExtUnix},
    };
    use std::time::{Duration, Instant};

    let mut builder = EventLoopBuilder::<()>::new();
    builder.with_any_thread(true);
    let mut events = builder.build();
    let window = crate::main_window_builder(false)
        .with_title("OxDM theater stacking test")
        .build(&events)
        .unwrap();
    for theater in [false, true, false] {
        crate::set_main_window_theater(&window, theater);
        let deadline = Instant::now() + Duration::from_secs(5);
        events.run_return(|_, _, flow| {
            *flow = if window.is_always_on_top() == theater || Instant::now() >= deadline {
                ControlFlow::Exit
            } else {
                ControlFlow::WaitUntil(deadline)
            };
        });
        assert_eq!(window.is_always_on_top(), theater);
    }
}
