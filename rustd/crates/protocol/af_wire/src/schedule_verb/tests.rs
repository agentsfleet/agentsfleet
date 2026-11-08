//! The schedules verb's bodies: what a runner may send, and what it may not.
//!
//! Parsed from text, never from a `serde_json::Value`: the text fields borrow
//! from the body they are read out of, as the daemon reads them.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use super::{ScheduleCreateRequest, SchedulePatchRequest, ScheduleRunRequest};

#[test]
fn a_create_naming_only_the_required_fields_defaults_the_rest() {
    let body = r#"{"fencing_token":7,"cron":"0 9 * * 1","message":"weekly check"}"#;
    let parsed: ScheduleCreateRequest<'_> =
        serde_json::from_str(body).expect("the required fields are enough");
    assert_eq!(parsed.fencing_token, 7);
    assert_eq!(
        parsed.timezone, None,
        "an absent zone is the daemon's default"
    );
    assert!(!parsed.once, "an absent `once` is a recurring schedule");
}

/// The fleet is the lease's: a body that names one is refused, not ignored.
#[test]
fn a_create_naming_a_fleet_is_refused() {
    let body = r#"{"fencing_token":7,"cron":"0 9 * * 1","message":"m","fleet_id":"x"}"#;
    let _refused =
        serde_json::from_str::<ScheduleCreateRequest<'_>>(body).expect_err("the body is refused");
}

#[test]
fn a_create_round_trips_every_field() {
    let text = r#"{"fencing_token":7,"cron":"0 9 * * 1","timezone":"Asia/Kolkata","message":"weekly check","once":true}"#;
    let parsed: ScheduleCreateRequest<'_> = serde_json::from_str(text).expect("it parses");
    assert_eq!(parsed.timezone.as_deref(), Some("Asia/Kolkata"));
    assert!(parsed.once);
    assert_eq!(
        serde_json::to_string(&parsed).expect("it serializes"),
        text,
        "what the runner sends is what the daemon reads"
    );
}

#[test]
fn a_patch_carrying_only_the_fence_changes_nothing() {
    let parsed: SchedulePatchRequest<'_> =
        serde_json::from_str(r#"{"fencing_token":7}"#).expect("the fence alone parses");
    assert_eq!(
        (parsed.cron, parsed.timezone, parsed.message, parsed.paused),
        (None, None, None, None)
    );
}

/// A patch cannot set `deleting`: only `paused` is spelled, so a delete is
/// the only way a fleet retires a schedule.
#[test]
fn a_patch_naming_a_desired_status_is_refused() {
    let body = r#"{"fencing_token":7,"desired_status":"deleting"}"#;
    let _refused =
        serde_json::from_str::<SchedulePatchRequest<'_>>(body).expect_err("the body is refused");
}

#[test]
fn a_run_request_carries_the_fence_and_nothing_else() {
    let parsed: ScheduleRunRequest =
        serde_json::from_str(r#"{"fencing_token":7}"#).expect("the fence parses");
    assert_eq!(parsed.fencing_token, 7);
    let extra = r#"{"fencing_token":7,"schedule_id":"x"}"#;
    let _refused =
        serde_json::from_str::<ScheduleRunRequest>(extra).expect_err("the body is refused");
}
