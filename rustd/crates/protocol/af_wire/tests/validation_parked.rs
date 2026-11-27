//! A runner's parked sandboxes on the wire: a beat names them every time,
//! the daemon's answer names its releases every time, a report can name no
//! park, and the list a runner sends is bounded.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a body that will not decode is an unmet precondition"
)]

use std::borrow::Cow;

use af_wire::report::ReportRequest;
use af_wire::runner::{
    FLEET_ID_TEXT_BYTES, HeartbeatRequest, HeartbeatResponse, PARKED_MAX, ParkedFleets,
};
use garde::Validate as _;

/// A fleet identifier as a runner sends it.
const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
/// A report from a runner that parks nothing.
const REPORT: &str = r#"{"run_id":"run","turn_id":"1-0","fencing_token":7,
    "outcome":"processed","failure_reason":null,"failure_detail":"",
    "response_text":"done","tokens":3,"requests":[],"telemetry":{"time_to_first_token_ms":4,"wall_ms":5},
    "checkpoint":{"last_turn_id":"1-0","last_response":""}}"#;
/// The instant a parking runner's report names, as it writes it.
const PARKED_UNTIL: &str = "2026-10-18T06:10:00.250Z";
/// A heartbeat answer releasing nothing.
// pin test: literal is the contract
const BEAT_ANSWER: &str = r#"{
  "status": "ok",
  "assigned_policy": {
    "sandbox_tier": "dev_none",
    "network_policy": "allow_all",
    "registry_allowlist": [],
    "worker_count": 1,
    "extra_binds": [],
    "durable_tasks_enabled": true
  },
  "degraded": false,
  "degraded_reason": null,
  "selftest_requested": false,
  "release_parked": []
}"#;

fn parked(count: usize, fleet: &str) -> ParkedFleets<'_> {
    ParkedFleets(vec![Cow::Borrowed(fleet); count])
}

#[test]
fn test_a_beat_names_its_parked_fleets_its_host_and_whether_it_is_closing() {
    for partial in [
        "{}",
        r#"{"parked":[]}"#,
        r#"{"closing":false}"#,
        r#"{"parked":[],"closing":false}"#,
    ] {
        assert!(
            serde_json::from_str::<HeartbeatRequest<'_>>(partial).is_err(),
            "{partial}"
        );
    }
    let beat: HeartbeatRequest<'_> =
        serde_json::from_str(r#"{"parked":[],"closing":false,"host":{}}"#).unwrap();
    assert_eq!(beat.parked, ParkedFleets::default());
    assert!(!beat.closing);
}

#[test]
fn a_report_naming_no_park_parks_nothing() {
    let report: ReportRequest<'_> = serde_json::from_str(REPORT).unwrap();
    assert_eq!(report.parked_until, None);
}

/// The daemon's answer names its releases and the assignment on every beat:
/// an answer missing either is refused rather than read as nothing released
/// or nothing assigned.
#[test]
fn test_a_beat_answer_names_its_releases_and_its_assignment() {
    let answer: HeartbeatResponse<'_> = serde_json::from_str(BEAT_ANSWER).unwrap();
    assert_eq!(answer.release_parked, [] as [Cow<'_, str>; 0]);

    let whole: serde_json::Value = serde_json::from_str(BEAT_ANSWER).unwrap();
    for field in ["release_parked", "assigned_policy"] {
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
fn test_a_report_that_parks_nothing_writes_no_park() {
    let report: ReportRequest<'_> = serde_json::from_str(REPORT).unwrap();

    let written = serde_json::to_value(&report).unwrap();

    assert!(written.get("parked_until").is_none(), "{written}");
}

#[test]
fn test_a_parked_deadline_round_trips() {
    let parked = REPORT.replace(
        r#""run_id""#,
        &format!(r#""parked_until":"{PARKED_UNTIL}","run_id""#),
    );

    let report: ReportRequest<'_> = serde_json::from_str(&parked).unwrap();
    let written = serde_json::to_value(&report).unwrap();

    let rewritten = written
        .get("parked_until")
        .and_then(serde_json::Value::as_str);

    // By value: jiff writes the shortest fraction, so `.250Z` comes back `.25Z`.
    let sent: jiff::Timestamp = PARKED_UNTIL.parse().unwrap();
    assert_eq!(report.parked_until, Some(sent));
    assert_eq!(rewritten.map(|text| text.parse().unwrap()), Some(sent));
}

#[test]
fn test_a_runner_parks_at_most_one_fleet_per_worker() {
    parked(PARKED_MAX, FLEET).validate().unwrap();

    assert!(parked(PARKED_MAX + 1, FLEET).validate().is_err());
}

#[test]
fn test_every_parked_fleet_is_an_identifiers_length() {
    let short = &FLEET[..FLEET_ID_TEXT_BYTES - 1];
    let long = format!("{FLEET}0");

    parked(1, FLEET).validate().unwrap();
    assert!(parked(1, short).validate().is_err());
    assert!(parked(1, &long).validate().is_err());
    assert!(parked(1, "").validate().is_err());
}

#[test]
fn a_heartbeat_names_its_parked_fleets_and_refuses_an_unknown_field() {
    let body = format!(r#"{{"parked":["{FLEET}"],"closing":false,"host":{{}}}}"#);
    let beat: HeartbeatRequest<'_> = serde_json::from_str(&body).unwrap();
    assert_eq!(beat.parked, parked(1, FLEET));
    beat.parked.validate().unwrap();
    let _ = serde_json::from_str::<HeartbeatRequest<'_>>(r#"{"future":1}"#).unwrap_err();
}

/// The bounds garde enforces on a runner's parked list, as a schema publishes them:
/// at most [`PARKED_MAX`] entries, each exactly an identifier's length.
fn assert_parked_bounded(body: &str, schema: &serde_json::Value) {
    let bound = |pointer: &str| schema.pointer(pointer).and_then(serde_json::Value::as_u64);
    let entry = u64::try_from(FLEET_ID_TEXT_BYTES).ok();
    assert_eq!(
        bound("/properties/parked/maxItems"),
        u64::try_from(PARKED_MAX).ok(),
        "{body}: {schema}"
    );
    assert_eq!(
        bound("/properties/parked/items/minLength"),
        entry,
        "{body}: {schema}"
    );
    assert_eq!(
        bound("/properties/parked/items/maxLength"),
        entry,
        "{body}: {schema}"
    );
}

/// The heartbeat publishes the same bounds it enforces on a runner's parked list.
#[cfg(feature = "openapi")]
#[test]
fn test_the_published_parked_bound_is_the_enforced_one() {
    let beat =
        serde_json::to_value(<HeartbeatRequest<'_> as utoipa::PartialSchema>::schema()).unwrap();

    assert_parked_bounded("HeartbeatRequest", &beat);
}
