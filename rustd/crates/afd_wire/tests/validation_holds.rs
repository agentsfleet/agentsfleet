//! A runner's held sandboxes on the wire: a beat names its holds every time,
//! the daemon's answer names its releases every time, a report or poll that
//! names none decodes as nothing held, and the list a runner sends is bounded.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a body that will not decode is an unmet precondition"
)]

use std::borrow::Cow;

use afd_wire::lease::{AcceptedKinds, LeaseRequest};
use afd_wire::report::ReportRequest;
use afd_wire::runner::{
    FLEET_ID_TEXT_BYTES, HOLDS_MAX, HeartbeatRequest, HeartbeatResponse, HeldFleets,
};
use garde::Validate as _;

/// A fleet identifier as a runner sends it.
const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
/// A report from a runner that holds nothing.
const REPORT: &str = r#"{"lease_id":"lease","event_id":"1-0","fencing_token":7,
    "outcome":"processed","failure_reason":null,"failure_detail":"",
    "response_text":"done","tokens":3,"input_tokens":1,"cached_input_tokens":0,
    "output_tokens":2,"telemetry":{"time_to_first_token_ms":4,"wall_ms":5},
    "checkpoint":{"last_event_id":"1-0","last_response":""}}"#;
/// The instant a holding runner's report names, as it writes it.
const HELD_UNTIL: &str = "2026-10-18T06:10:00.250Z";
/// A heartbeat answer releasing nothing.
// pin test: literal is the contract
const BEAT_ANSWER: &str = r#"{"heartbeat_interval_ms":10000,"status":"ok",
    "assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all",
    "registry_allowlist":[],"worker_count":1,"extra_binds":[]},
    "degraded":false,"degraded_reason":null,"selftest_requested":false,"release_holds":[]}"#;

fn held(count: usize, fleet: &str) -> HeldFleets<'_> {
    HeldFleets(vec![Cow::Borrowed(fleet); count])
}

#[test]
fn test_a_beat_names_its_holds_and_whether_it_is_closing() {
    for partial in ["{}", r#"{"holds":[]}"#, r#"{"closing":false}"#] {
        assert!(
            serde_json::from_str::<HeartbeatRequest<'_>>(partial).is_err(),
            "{partial}"
        );
    }
    let beat: HeartbeatRequest<'_> =
        serde_json::from_str(r#"{"holds":[],"closing":false}"#).unwrap();
    assert_eq!(beat.holds, HeldFleets::default());
    assert!(!beat.closing);
}

#[test]
fn test_a_report_or_poll_naming_no_hold_holds_nothing() {
    let report: ReportRequest<'_> = serde_json::from_str(REPORT).unwrap();
    let poll: LeaseRequest<'_> = serde_json::from_str(r#"{"accepted_kinds":["fleet"]}"#).unwrap();

    assert_eq!(report.held_until, None);
    assert_eq!(poll.holds, HeldFleets::default());
    assert_eq!(poll.accepted_kinds, AcceptedKinds::FLEET);
}

/// The daemon's answer names its releases and the assignment on every beat:
/// an answer missing either is refused rather than read as nothing released
/// or nothing assigned.
#[test]
fn test_a_beat_answer_names_its_releases_and_its_assignment() {
    let answer: HeartbeatResponse<'_> = serde_json::from_str(BEAT_ANSWER).unwrap();
    assert_eq!(answer.release_holds, [] as [Cow<'_, str>; 0]);

    let whole: serde_json::Value = serde_json::from_str(BEAT_ANSWER).unwrap();
    for field in ["release_holds", "assigned_policy"] {
        let mut absent = whole.clone();
        absent.as_object_mut().unwrap().remove(field);
        let mut null = whole.clone();
        null.as_object_mut()
            .unwrap()
            .insert(field.to_owned(), serde_json::Value::Null);
        for broken in [absent.to_string(), null.to_string()] {
            assert!(
                serde_json::from_str::<HeartbeatResponse<'_>>(&broken).is_err(),
                "{broken}"
            );
        }
    }
}

#[test]
fn test_a_report_that_holds_nothing_writes_no_hold() {
    let report: ReportRequest<'_> = serde_json::from_str(REPORT).unwrap();

    let written = serde_json::to_value(&report).unwrap();

    assert!(written.get("held_until").is_none(), "{written}");
}

#[test]
fn test_a_held_deadline_round_trips() {
    let held = REPORT.replace(
        r#""lease_id""#,
        &format!(r#""held_until":"{HELD_UNTIL}","lease_id""#),
    );

    let report: ReportRequest<'_> = serde_json::from_str(&held).unwrap();
    let written = serde_json::to_value(&report).unwrap();

    let rewritten = written
        .get("held_until")
        .and_then(serde_json::Value::as_str);

    // By value: jiff writes the shortest fraction, so `.250Z` comes back `.25Z`.
    let sent: jiff::Timestamp = HELD_UNTIL.parse().unwrap();
    assert_eq!(report.held_until, Some(sent));
    assert_eq!(rewritten.map(|text| text.parse().unwrap()), Some(sent));
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
    let body = format!(r#"{{"holds":["{FLEET}"],"accepted_kinds":["fleet"]}}"#);

    let poll: LeaseRequest<'_> = serde_json::from_str(&body).unwrap();

    assert_eq!(poll.holds, held(1, FLEET));
    poll.validate().unwrap();
    let _ = serde_json::from_str::<LeaseRequest<'_>>(r#"{"future":1}"#).unwrap_err();
}

/// The bounds garde enforces on a runner's holds, as a schema publishes them:
/// at most [`HOLDS_MAX`] entries, each exactly an identifier's length.
fn assert_holds_bounded(body: &str, schema: &serde_json::Value) {
    let bound = |pointer: &str| schema.pointer(pointer).and_then(serde_json::Value::as_u64);
    let entry = u64::try_from(FLEET_ID_TEXT_BYTES).ok();
    assert_eq!(
        bound("/properties/holds/maxItems"),
        u64::try_from(HOLDS_MAX).ok(),
        "{body}: {schema}"
    );
    assert_eq!(
        bound("/properties/holds/items/minLength"),
        entry,
        "{body}: {schema}"
    );
    assert_eq!(
        bound("/properties/holds/items/maxLength"),
        entry,
        "{body}: {schema}"
    );
}

/// Both bodies that carry a runner's holds derive the bounds garde enforces.
/// utoipa takes only a literal for them, so this keeps the two spellings one.
#[cfg(feature = "openapi")]
#[test]
fn test_the_published_holds_bound_is_the_enforced_one() {
    let poll = serde_json::to_value(<LeaseRequest<'_> as utoipa::PartialSchema>::schema()).unwrap();
    let beat =
        serde_json::to_value(<HeartbeatRequest<'_> as utoipa::PartialSchema>::schema()).unwrap();

    assert_holds_bounded("LeaseRequest", &poll);
    assert_holds_bounded("HeartbeatRequest", &beat);
}
