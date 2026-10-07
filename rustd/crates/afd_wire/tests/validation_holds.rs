//! A runner's held sandboxes on the wire: every field that names one decodes
//! as nothing held when absent, so a runner or daemon from before them still
//! reads and writes as it did, and the list a runner sends is bounded.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a body that will not decode is an unmet precondition"
)]

use std::borrow::Cow;

use afd_wire::lease::LeaseRequest;
use afd_wire::report::ReportRequest;
use afd_wire::runner::{
    FLEET_ID_TEXT_BYTES, HOLDS_MAX, HeartbeatRequest, HeartbeatResponse, HeldFleets,
};
use garde::Validate as _;

/// A fleet identifier as a runner sends it.
const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
/// A report from a runner that holds nothing, or predates holding.
const REPORT: &str = r#"{"lease_id":"lease","event_id":"1-0","fencing_token":7,
    "outcome":"processed","failure_reason":null,"failure_detail":"",
    "response_text":"done","tokens":3,"input_tokens":1,"cached_input_tokens":0,
    "output_tokens":2,"telemetry":{"time_to_first_token_ms":4,"wall_ms":5},
    "checkpoint":{"last_event_id":"1-0","last_response":""}}"#;
/// A heartbeat answer from a daemon that predates holding.
// pin test: literal is the contract
const BEAT_ANSWER: &str = r#"{"heartbeat_interval_ms":10000,"status":"ok","assigned_policy":null,
    "degraded":false,"degraded_reason":null,"selftest_requested":false}"#;

fn held(count: usize, fleet: &str) -> HeldFleets<'_> {
    HeldFleets(vec![Cow::Borrowed(fleet); count])
}

#[test]
fn test_hold_fields_are_optional_on_the_wire() {
    let report: ReportRequest<'_> = serde_json::from_str(REPORT).unwrap();
    let beat: HeartbeatRequest<'_> = serde_json::from_str("{}").unwrap();
    let answer: HeartbeatResponse<'_> = serde_json::from_str(BEAT_ANSWER).unwrap();
    let poll: LeaseRequest<'_> = serde_json::from_str("{}").unwrap();

    assert_eq!(report.held_until_ms, None);
    assert_eq!(beat.holds, HeldFleets::default());
    assert_eq!(answer.release_holds, [] as [Cow<'_, str>; 0]);
    assert_eq!(poll, LeaseRequest::default());
}

#[test]
fn test_a_report_that_holds_nothing_writes_no_hold() {
    let report: ReportRequest<'_> = serde_json::from_str(REPORT).unwrap();

    let written = serde_json::to_value(&report).unwrap();

    assert!(written.get("held_until_ms").is_none(), "{written}");
}

#[test]
fn test_a_held_deadline_round_trips() {
    let held = REPORT.replace(r#""lease_id""#, r#""held_until_ms":600000,"lease_id""#);

    let report: ReportRequest<'_> = serde_json::from_str(&held).unwrap();

    assert_eq!(report.held_until_ms, Some(600_000));
}

#[test]
fn test_a_runner_holds_at_most_one_fleet_per_worker() {
    held(HOLDS_MAX, FLEET).validate().unwrap();

    assert!(held(HOLDS_MAX + 1, FLEET).validate().is_err());
}

#[test]
fn test_every_held_fleet_is_an_identifiers_length() {
    let short = &FLEET[..FLEET_ID_TEXT_BYTES - 1];
    let long = format!("{FLEET}0");

    held(1, FLEET).validate().unwrap();
    assert!(held(1, short).validate().is_err());
    assert!(held(1, &long).validate().is_err());
    assert!(held(1, "").validate().is_err());
}

#[test]
fn test_a_poll_naming_its_holds_reads_them_and_refuses_a_stranger() {
    let body = format!(r#"{{"holds":["{FLEET}"]}}"#);

    let poll: LeaseRequest<'_> = serde_json::from_str(&body).unwrap();

    assert_eq!(poll.holds, held(1, FLEET));
    poll.validate().unwrap();
    let _ = serde_json::from_str::<LeaseRequest<'_>>(r#"{"future":1}"#).unwrap_err();
}

/// Both bodies that carry a runner's holds publish the bounds garde enforces:
/// the list's length and each entry's. utoipa takes only a literal for them,
/// so this keeps the two spellings one.
#[cfg(feature = "openapi")]
#[test]
#[expect(
    clippy::indexing_slicing,
    reason = "a schema missing the property indexes to null, which the assertion then names"
)]
fn test_the_published_holds_bound_is_the_enforced_one() {
    let poll = serde_json::to_value(<LeaseRequest<'_> as utoipa::PartialSchema>::schema()).unwrap();
    let beat =
        serde_json::to_value(<HeartbeatRequest<'_> as utoipa::PartialSchema>::schema()).unwrap();

    let entry = u64::try_from(FLEET_ID_TEXT_BYTES).ok();
    for schema in [poll, beat] {
        let holds = &schema["properties"]["holds"];
        assert_eq!(
            holds["maxItems"].as_u64(),
            u64::try_from(HOLDS_MAX).ok(),
            "{schema}"
        );
        assert_eq!(holds["items"]["minLength"].as_u64(), entry, "{schema}");
        assert_eq!(holds["items"]["maxLength"].as_u64(), entry, "{schema}");
    }
}
