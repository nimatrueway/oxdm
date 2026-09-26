//! End-to-end smoke test for OxDM's focus wiring against the oxvif mock.
//!
//! The unit tests in `src/tests/api_tests.rs` cover the *decisions* —
//! `pick_channel` and `focus_speed` — over hand-built values. They cannot say
//! whether OxDM's wrappers reach the device at all, or whether the numbers they
//! decide from are the numbers a camera actually sends. That is this file.
//!
//! It leans on one property of oxvif's mock: it is a **two-sensor** device
//! whose lenses deliberately disagree. `VS_1` has a motorised focus; `VS_2` is
//! fixed and *faults* on `GetMoveOptions` rather than reporting a range it does
//! not have. A single-sensor fixture cannot express the difference between "the
//! device supports focus" and "*this channel* supports focus", which is the
//! whole question a multi-sensor camera raises.

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
async fn unsupported_snapshot_uri_is_remembered_for_its_profile() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url();
    let creds = Credentials::default();
    server.inject_fault("GetSnapshotUri", "SOAP-ENV:Receiver", "");

    let first = api::get_snapshot_uri(addr, &creds, "Profile_1")
        .await
        .unwrap_err();
    let repeated = api::get_snapshot_uri(addr, &creds, "Profile_1")
        .await
        .expect_err("an unsupported snapshot must not be probed again");
    assert_eq!(repeated, first);
    assert!(api::get_snapshot_uri(addr, &creds, "Profile_2")
        .await
        .is_ok());
    assert!(api::imaging_get_status(addr, &creds, "VS_1").await.is_ok());

    sessions::invalidate(addr);
    assert!(api::get_snapshot_uri(addr, &creds, "Profile_1")
        .await
        .is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn unsupported_imaging_status_is_remembered_for_its_source() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url();
    let creds = Credentials::default();
    server.inject_fault(
        "GetStatus",
        "ter:ActionNotSupported",
        "Status not supported",
    );

    let first = api::imaging_get_status(addr, &creds, "VS_1")
        .await
        .unwrap_err();
    let repeated = api::imaging_get_status(addr, &creds, "VS_1")
        .await
        .expect_err("unsupported imaging status must not be probed again");
    assert_eq!(repeated, first);
    assert!(api::imaging_get_status(addr, &creds, "VS_2").await.is_ok());
    assert!(api::get_snapshot_uri(addr, &creds, "Profile_1")
        .await
        .is_ok());

    sessions::invalidate(addr);
    assert!(api::imaging_get_status(addr, &creds, "VS_1").await.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn unsupported_optional_operations_share_concurrent_probes() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url();
    let creds = Credentials::default();

    server.inject_fault("GetSnapshotUri", "ter:ActionNotSupported", "");
    let (first, concurrent) = tokio::join!(
        api::get_snapshot_uri(addr, &creds, "Profile_1"),
        api::get_snapshot_uri(addr, &creds, "Profile_1"),
    );
    assert_eq!(first.unwrap_err(), concurrent.unwrap_err());

    server.inject_fault("GetStatus", "SOAP-ENV:Receiver", "");
    let (first, concurrent) = tokio::join!(
        api::imaging_get_status(addr, &creds, "VS_1"),
        api::imaging_get_status(addr, &creds, "VS_1"),
    );
    assert_eq!(first.unwrap_err(), concurrent.unwrap_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn optional_operations_retry_transient_and_authorization_faults() {
    let server = MockServer::start().await.unwrap();
    let addr = server.device_url();
    let creds = Credentials::default();
    for (code, reason) in [
        ("s:Receiver", "Device busy"),
        ("ter:NotAuthorized", "Operation not supported for this user"),
    ] {
        server.inject_fault("GetSnapshotUri", code, reason);
        assert!(api::get_snapshot_uri(addr, &creds, "Profile_1")
            .await
            .is_err());
        assert!(api::get_snapshot_uri(addr, &creds, "Profile_1")
            .await
            .is_ok());

        server.inject_fault("GetStatus", code, reason);
        assert!(api::imaging_get_status(addr, &creds, "VS_1").await.is_err());
        assert!(api::imaging_get_status(addr, &creds, "VS_1").await.is_ok());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn focus_speed_comes_from_what_the_lens_declared() {
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    // The motorised lens declares a symmetric continuous range. Asserting the
    // numbers, not just `is_ok()` — the point of the fix is that these values
    // reach the speed calculation, and a hollow assertion here would leave the
    // whole chain unproven.
    let opts = api::imaging_get_move_options(&addr, &creds, "VS_1")
        .await
        .expect("motorised lens answers GetMoveOptions");
    let continuous = opts
        .continuous_speed_range
        .expect("the mock declares tt:Continuous/Speed");
    assert_eq!(continuous.min, -1.0);
    assert_eq!(continuous.max, 1.0);

    // Absolute focus is declared too, and its members are `Position`/`Speed` —
    // not the `PositionSpace`/`SpeedSpace` spelling that made every range read
    // as absent through oxvif 0.14.
    let position = opts
        .absolute_position_range
        .expect("tt:Absolute/Position is required by the schema when the family is present");
    assert_eq!(position.max, 1.0);

    // Drive the real declared range through the speed calculation.
    assert_eq!(api::focus_speed(continuous, 0.5, 1.0), Some(0.5));
    assert_eq!(api::focus_speed(continuous, 0.5, -1.0), Some(-0.5));

    // And the motor accepts the value that came out of it.
    api::imaging_focus_continuous(&addr, &creds, "VS_1", 0.5)
        .await
        .expect("continuous focus move at a declared speed");
    api::imaging_focus_stop(&addr, &creds, "VS_1")
        .await
        .expect("focus stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fixed_lens_is_a_fault_not_an_empty_range() {
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    let err = api::imaging_get_move_options(&addr, &creds, "VS_2")
        .await
        .expect_err("the fixed lens refuses GetMoveOptions");

    // The exact fault the mock sends, so this cannot pass on any other error —
    // an auth failure or a routing mistake would otherwise look identical.
    assert!(
        err.contains("NoFocusSupport-IMGMOVEOPT-5611"),
        "expected the fixed-lens fault, got: {err}"
    );

    // The distinction the view depends on: a *failed query* must not disable
    // the focus buttons the way a successful "no continuous family" answer
    // does. Here the error is what reaches the view, and it keeps driving the
    // motor the way it always has rather than hiding a control on no evidence.
    let motorised = api::imaging_get_move_options(&addr, &creds, "VS_1").await;
    assert!(
        motorised.is_ok(),
        "the other lens on the same device must still answer"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn imaging_status_reports_a_position_for_the_motorised_lens_only() {
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    let one = api::imaging_get_status(&addr, &creds, "VS_1")
        .await
        .expect("GetStatus on the motorised lens");
    assert!(
        one.focus_position.is_some(),
        "the motorised lens reports a focus position"
    );

    // `tt:ImagingStatus20` has `FocusStatus20` as its only content and it is
    // [0..1], so a legal response can carry no position at all. The readout has
    // to survive that rather than showing a zero.
    let two = api::imaging_get_status(&addr, &creds, "VS_2")
        .await
        .expect("GetStatus on the fixed lens is still a valid response");
    assert_eq!(
        two.focus_position, None,
        "a fixed lens has no focus position to report"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_two_lenses_resolve_to_different_channels() {
    // Item 10's integration half: `pick_channel` over the profiles a real
    // (mock) device sends, rather than over a hand-built list.
    let server = MockServer::start().await.expect("mock server boots");
    let addr = server.device_url().to_string();
    let creds = Credentials::default();

    let profiles = api::get_profiles(&addr, &creds).await.expect("GetProfiles");
    assert!(
        profiles.len() >= 2,
        "this test needs a multi-profile device, got {}",
        profiles.len()
    );

    // Collect the distinct video sources the device's profiles point at. The
    // mock is a two-sensor camera, so there must be more than one — if this
    // ever collapses to a single source the fixture has stopped being able to
    // catch a token-blind caller, and every per-channel test above it becomes
    // decorative.
    let sources: std::collections::BTreeSet<_> = profiles
        .iter()
        .filter_map(|p| p.video_source_token.clone())
        .collect();
    assert!(
        sources.len() >= 2,
        "the mock must expose two video sources for this to prove anything, got {sources:?}"
    );

    // Asking for a profile bound to a given source resolves to *that* source,
    // not to the first one on the device.
    for p in profiles.iter().filter(|p| p.video_source_token.is_some()) {
        let pick = api::pick_channel(&profiles, Some(&p.token), api::ChannelKind::Source)
            .expect("a profile with a video source resolves");
        assert_eq!(
            Some(&pick.token),
            p.video_source_token.as_ref(),
            "profile {} resolved to the wrong video source",
            p.token
        );
        assert!(!pick.fell_back, "profile {} was not a fallback", p.token);
    }
}
