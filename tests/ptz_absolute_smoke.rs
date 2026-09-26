//! End-to-end smoke test for OxDM's PTZ status / node / absolute-move wiring
//! against the oxvif mock.
//!
//! The mock is a **two-head** device and the heads deliberately disagree, which
//! is what makes this able to fail:
//!
//! ```text
//! Profile_1, Profile_2  →  PTZConfig_1  →  PTZNode_1   pan + tilt + zoom
//! Profile_3             →  PTZConfig_2  →  PTZNode_2   zoom only, no pan/tilt space
//! Profile_4             →  (unbound)                   no PTZ at all
//! ```
//!
//! A resolver that ignored the profile token, or that fell back to the first
//! node when a lookup missed, would answer `PTZNode_1` for all four and pass
//! every assertion a single-head fixture could make.

use oxvif::mock::MockServer;

#[path = "../src/api.rs"]
#[allow(dead_code, unused_imports)]
mod api;
#[path = "../src/sessions.rs"]
#[allow(dead_code, unused_imports)]
mod sessions;
#[path = "../src/state.rs"]
#[allow(dead_code, unused_imports)]
mod state;

use crate::state::Credentials;

#[tokio::test(flavor = "multi_thread")]
async fn drag_pan_stops_when_pointer_updates_stop_before_release() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url().to_string();
    let creds = Credentials::default();
    server.inject_fault("Stop", "ter:ActionNotSupported", "idle-stop-observed");
    let (updates, receiver) = tokio::sync::watch::channel((0.5, 0.0));
    let worker = tokio::spawn(api::ptz_drag(
        addr,
        creds,
        "Profile_1".to_string(),
        receiver,
    ));

    let error = tokio::time::timeout(std::time::Duration::from_secs(1), worker)
        .await
        .expect("an idle pointer must stop the camera without waiting for release")
        .unwrap()
        .unwrap_err();
    assert!(error.contains("idle-stop-observed"), "{error}");
    drop(updates);
}

#[tokio::test(flavor = "multi_thread")]
async fn drag_pan_resumes_only_after_more_pointer_movement() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url().to_string();
    let creds = Credentials::default();
    let mut previous = api::ptz_get_status(&addr, &creds, "Profile_1")
        .await
        .unwrap()
        .pan;
    let (updates, receiver) = tokio::sync::watch::channel((0.0, 0.0));
    let worker = tokio::spawn(api::ptz_drag(
        addr.clone(),
        creds.clone(),
        "Profile_1".to_string(),
        receiver,
    ));

    for _ in 0..2 {
        updates.send((0.5, 0.0)).unwrap();
        let moved = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let current = api::ptz_get_status(&addr, &creds, "Profile_1")
                    .await
                    .unwrap();
                if current.pan != previous {
                    break current.pan;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("fresh pointer movement must restart panning");
        assert!((moved.unwrap() - previous.unwrap() - 0.025).abs() < 0.000001);
        tokio::time::sleep(std::time::Duration::from_millis(375)).await;
        let held = api::ptz_get_status(&addr, &creds, "Profile_1")
            .await
            .unwrap();
        assert_eq!(held.pan, moved, "holding still must not issue another move");
        previous = moved;
    }

    drop(updates);
    tokio::time::timeout(std::time::Duration::from_secs(3), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn drag_pan_release_ends_motion_without_queued_commands() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url().to_string();
    let creds = Credentials::default();
    for (horizontal, vertical) in [
        (52.0, 0.0),
        (0.0, -52.0),
        (0.0, 52.0),
        (52.0, 8.0),
        (8.0, -52.0),
        (-8.0, 52.0),
    ] {
        let before = api::ptz_get_status(&addr, &creds, "Profile_1")
            .await
            .unwrap();
        let velocity = api::ptz_drag_velocity(horizontal, vertical, 0.5);
        let (updates, receiver) = tokio::sync::watch::channel(velocity);
        let worker = tokio::spawn(api::ptz_drag(
            addr.clone(),
            creds.clone(),
            "Profile_1".to_string(),
            receiver,
        ));
        let moved = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let current = api::ptz_get_status(&addr, &creds, "Profile_1")
                    .await
                    .unwrap();
                if current.pan != before.pan || current.tilt != before.tilt {
                    break current;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(375)).await;
        let held = api::ptz_get_status(&addr, &creds, "Profile_1")
            .await
            .unwrap();
        drop(updates);
        tokio::time::timeout(std::time::Duration::from_secs(3), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            held.pan, moved.pan,
            "steady hold must not repeat pan commands"
        );
        assert_eq!(
            held.tilt, moved.tilt,
            "steady hold must not repeat tilt commands"
        );
        if velocity.0 != 0.0 {
            assert!(moved.pan > before.pan);
            assert_eq!(moved.tilt, before.tilt);
        } else {
            assert_eq!(moved.pan, before.pan);
            if vertical < 0.0 {
                assert!(moved.tilt > before.tilt);
            } else {
                assert!(moved.tilt < before.tilt);
            }
        }
        let stopped = api::ptz_get_status(&addr, &creds, "Profile_1")
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(175)).await;
        let after = api::ptz_get_status(&addr, &creds, "Profile_1")
            .await
            .unwrap();
        assert_eq!(after.pan, stopped.pan);
        assert_eq!(after.tilt, stopped.tilt);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn drag_pan_filters_jitter_without_losing_speed_changes_or_reversals() {
    const MOCK_MOVE_STEP: f32 = 0.05;
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url().to_string();
    let creds = Credentials::default();
    let mut before = api::ptz_get_status(&addr, &creds, "Profile_1")
        .await
        .unwrap();
    let (updates, receiver) = tokio::sync::watch::channel((0.0, 0.0));
    let worker = tokio::spawn(api::ptz_drag(
        addr.clone(),
        creds.clone(),
        "Profile_1".to_string(),
        receiver,
    ));
    for (velocity, should_move) in [
        ((0.5, 0.0), true),
        ((0.5, 0.0), false),
        ((0.54, 0.0), false),
        ((0.47, 0.0), false),
        ((0.58, 0.0), false),
        ((0.65, 0.0), true),
        ((0.67, 0.0), false),
        ((0.65, 0.02), true),
        ((0.65, -0.02), true),
        ((0.65, 0.0), true),
        ((0.0, 0.0), false),
        ((0.65, 0.0), true),
        ((0.02, 0.0), true),
        ((-0.02, 0.0), true),
    ] {
        updates.send(velocity).unwrap();
        if should_move {
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    let current = api::ptz_get_status(&addr, &creds, "Profile_1")
                        .await
                        .unwrap();
                    if current.pan != before.pan || current.tilt != before.tilt {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        }
        for _ in 0..5 {
            updates.send(velocity).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(75)).await;
        }
        let after = api::ptz_get_status(&addr, &creds, "Profile_1")
            .await
            .unwrap();
        let step = if should_move { MOCK_MOVE_STEP } else { 0.0 };
        assert!(
            (after.pan.unwrap() - before.pan.unwrap() - velocity.0 * step).abs() < 0.000001,
            "unexpected pan command count or speed for {velocity:?}"
        );
        assert!(
            (after.tilt.unwrap() - before.tilt.unwrap() - velocity.1 * step).abs() < 0.000001,
            "unexpected tilt command count or speed for {velocity:?}"
        );
        before = after;
    }
    server.inject_fault("Stop", "ter:ActionNotSupported", "release-stop-observed");
    drop(updates);
    let error = tokio::time::timeout(std::time::Duration::from_secs(3), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.contains("release-stop-observed"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn ptz_travel_tracks_the_selected_heads_reported_position() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url();
    let creds = Credentials::default();
    let node = api::ptz_node_for_profile(addr, &creds, "Profile_1")
        .await
        .unwrap();
    let limits = api::ptz_absolute_limits(&node);
    let before = api::ptz_get_status(addr, &creds, "Profile_1")
        .await
        .unwrap();
    assert_eq!(api::ptz_axis_fraction(before.pan, limits.pan), Some(0.5));
    assert_eq!(api::ptz_axis_fraction(before.tilt, limits.tilt), Some(0.5));

    api::ptz_absolute_move(addr, &creds, "Profile_1", 0.5, -0.5, 0.0)
        .await
        .unwrap();
    let now = api::ptz_get_status(addr, &creds, "Profile_1")
        .await
        .unwrap();
    let pan = api::ptz_axis_fraction(now.pan, limits.pan).unwrap();
    let tilt = api::ptz_axis_fraction(now.tilt, limits.tilt).unwrap();
    assert_eq!((pan, 1.0 - pan, 1.0 - tilt, tilt), (0.75, 0.25, 0.75, 0.25));

    let other_node = api::ptz_node_for_profile(addr, &creds, "Profile_3")
        .await
        .unwrap();
    let other_limits = api::ptz_absolute_limits(&other_node);
    let other_position = api::ptz_get_status(addr, &creds, "Profile_3")
        .await
        .unwrap();
    assert_eq!(
        api::ptz_axis_fraction(other_position.pan, other_limits.pan),
        None
    );
    assert_eq!(
        api::ptz_axis_fraction(other_position.tilt, other_limits.tilt),
        None
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn each_profile_resolves_to_its_own_head() {
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    let one = api::ptz_node_for_profile(&addr, &creds, "Profile_1")
        .await
        .expect("Profile_1 is bound to a PTZ configuration");
    assert_eq!(one.token, "PTZNode_1");

    let three = api::ptz_node_for_profile(&addr, &creds, "Profile_3")
        .await
        .expect("Profile_3 is bound to the second head");
    assert_eq!(
        three.token, "PTZNode_2",
        "Profile_3 must reach head 2 — answering head 1 here is the multi-head \
         bug this fixture exists to catch"
    );

    // The two heads disagree on more than their token, so a renderer that
    // ignored `NodeToken` could not get these right either.
    assert_ne!(one.max_presets, three.max_presets);
    assert!(one.home_supported);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_profile_with_no_ptz_configuration_resolves_to_nothing() {
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    let err = api::ptz_node_for_profile(&addr, &creds, "Profile_4")
        .await
        .expect_err("Profile_4 is deliberately unbound");

    // Specifically "this profile has no head", not a transport or fault error —
    // and above all not a silent fallback to head 1.
    assert_eq!(err, "no_ptz_config");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_zoom_only_head_offers_zoom_and_not_pan_or_tilt() {
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    let full = api::ptz_node_for_profile(&addr, &creds, "Profile_1")
        .await
        .unwrap();
    let full_limits = api::ptz_absolute_limits(&full);
    assert!(full_limits.pan.is_some(), "head 1 pans");
    assert!(full_limits.tilt.is_some(), "head 1 tilts");
    assert!(full_limits.zoom.is_some(), "head 1 zooms");

    let zoom_only = api::ptz_node_for_profile(&addr, &creds, "Profile_3")
        .await
        .unwrap();
    let zoom_limits = api::ptz_absolute_limits(&zoom_only);
    assert_eq!(zoom_limits.pan, None, "head 2 declares no pan/tilt space");
    assert_eq!(zoom_limits.tilt, None);
    assert!(
        zoom_limits.zoom.is_some(),
        "a zoom-only head still zooms — the whole block must not vanish"
    );
    assert!(!zoom_limits.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_absolute_move_is_visible_in_the_next_status_read() {
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    let before = api::ptz_get_status(&addr, &creds, "Profile_1")
        .await
        .expect("GetStatus on a bound profile");

    // Pick a target the head is not already at, so this cannot pass by the
    // device simply not moving.
    let target_pan = match before.pan {
        Some(p) if p > 0.0 => -0.5,
        _ => 0.5,
    };

    api::ptz_absolute_move(&addr, &creds, "Profile_1", target_pan, 0.25, 0.75)
        .await
        .expect("AbsoluteMove within the declared range");

    let after = api::ptz_get_status(&addr, &creds, "Profile_1")
        .await
        .expect("GetStatus after the move");

    assert_eq!(
        after.pan,
        Some(target_pan),
        "the head did not end up where it was sent"
    );
    assert_eq!(after.tilt, Some(0.25));
    assert_eq!(after.zoom, Some(0.75));

    // And the other head must not have followed it. This is the assertion that
    // makes the whole file a *multi-head* test rather than a round-trip one.
    let other = api::ptz_get_status(&addr, &creds, "Profile_3")
        .await
        .expect("GetStatus on the second head");
    assert_ne!(
        other.pan,
        Some(target_pan),
        "moving head 1 moved head 2 — the mock is not discriminating and \
         nothing above this line proves anything"
    );
}
