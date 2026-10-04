//! A capability report past its bounds is ignored, and the beat still lands.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_wire::runner::{CONTROLLER_NAME_MAX_BYTES, HeartbeatRequest, REPORT_CONTROLLERS_MAX};
use serde_json::json;

use super::bounded_report;

/// A beat body as a runner sends it, reporting `controllers`.
fn beat_body(controllers: &[String]) -> String {
    json!({
        "capability_report": {
            "landlock": true,
            "seccomp": true,
            "cgroup_controllers": controllers,
            "bubblewrap": true,
            "egress_enforcement": true,
        },
        "selftest": null,
    })
    .to_string()
}

/// Whether the beat parsed AND its report would be reconciled.
fn reported(controllers: &[String]) -> bool {
    let body = beat_body(controllers);
    let beat: HeartbeatRequest<'_> =
        serde_json::from_str(&body).expect("an out-of-bounds report still parses as a beat");
    bounded_report(&beat).is_some()
}

#[test]
fn test_capability_report_out_of_bounds_is_ignored() {
    let named = |count: usize| vec!["cpu".to_owned(); count];
    assert!(reported(&named(2)));
    assert!(reported(&named(REPORT_CONTROLLERS_MAX)));
    // Seventeen controllers: the beat parses and lands, the report is not one.
    assert!(!reported(&named(REPORT_CONTROLLERS_MAX + 1)));
    assert!(!reported(&[String::new()]));
    assert!(!reported(&["c".repeat(CONTROLLER_NAME_MAX_BYTES + 1)]));
}

#[test]
fn a_beat_without_a_report_reconciles_the_stored_one() {
    assert!(bounded_report(&super::NO_REPORT).is_none());
}
