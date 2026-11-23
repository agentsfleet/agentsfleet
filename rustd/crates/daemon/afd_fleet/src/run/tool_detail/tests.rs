#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and indexes what it built"
)]

use std::collections::BTreeSet;

use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_wire::tool_detail::{DetailRejection, ToolCallRecordsRequest};
use serde_json::json;

use super::{EVENT_SKIPPED, Skip, log_skip, named_turns, narrow_all, serving};
use crate::run::standing::LiveRun;

/// A post's body: `records` as given, under fence 7.
fn post(records: &[String]) -> String {
    format!(r#"{{"fencing_token":7,"calls":[{}]}}"#, records.join(","))
}

/// One record of `call_number` whose output is `output_bytes` long, a call
/// of the run's own turn.
fn record(call_number: u64, output_bytes: usize) -> String {
    of_turn(call_number, output_bytes, EVENT)
}

/// [`record`], naming `turn`.
fn of_turn(call_number: u64, output_bytes: usize, turn: &str) -> String {
    json!({"call_number": call_number, "arguments": {}, "truncated_arguments": false,
           "output": "a".repeat(output_bytes), "output_line_count": 1, "truncated": false,
           "turn_id": turn, "kind": "tool_call", "tool": "shell", "status": "succeeded"})
    .to_string()
}

/// The event every fixture run runs.
const EVENT: &str = "01924f4e-0000-7000-8000-0000000000e9";
/// A turn a person sent that the run's conversation folded in.
const FOLDED: &str = "01924f4e-0000-7000-8000-0000000000f0";
/// A turn of the same fleet the run does not serve.
const OTHER: &str = "01924f4e-0000-7000-8000-0000000000f1";

/// One KiB, the unit the size cases are written in.
const KIB: usize = 1024;

/// The turns a fixture run serves.
fn served(turns: &[&str]) -> BTreeSet<String> {
    turns.iter().map(|turn| (*turn).to_owned()).collect()
}

fn target(fence: i64, live_seq: i64) -> LiveRun {
    LiveRun {
        fleet_id: Uuid7::parse("01924f4e-0000-7000-8000-00000000fee7").expect("fixture fleet id"),
        workspace_id: Uuid7::parse("01924f4e-0000-7000-8000-000000000001")
            .expect("fixture workspace id"),
        turn_id: Uuid7::parse(EVENT).expect("fixture event id"),
        actor: "steer:api".to_owned(),
        fence,
        live_seq,
    }
}

#[test]
fn only_the_live_holder_of_its_own_fence_may_post() {
    assert!(target(7, 7).holds(7));
    assert!(!target(6, 7).holds(6), "a reassignment moved the fleet on");
    assert!(!target(7, 7).holds(6), "a token that is not the run's own");
    assert!(
        !target(-1, -1).holds(0),
        "a corrupt negative fence holds nothing"
    );
}

#[test]
fn each_record_is_narrowed_on_its_own() {
    let body = post(&[
        record(1, 4),
        "7".to_owned(),
        record(2, 200 * KIB),
        record(3, 4),
    ]);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (kept, skipped) = narrow_all(&request.calls);
    assert_eq!(kept.keys().copied().collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(
        kept[&2].record.output.len(),
        200 * KIB,
        "no field cap cuts it"
    );
    assert_eq!(
        skipped
            .iter()
            .map(|skip| (skip.position, skip.reason))
            .collect::<Vec<_>>(),
        [(1, DetailRejection::Malformed)]
    );
}

#[test]
fn a_call_named_twice_keeps_its_last_record() {
    let records = [record(4, 1), record(4, 9)];
    let body = post(&records);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (kept, skipped) = narrow_all(&request.calls);
    assert_eq!(kept[&4].position, 1);
    assert_eq!(
        skipped,
        [Skip {
            position: 0,
            reason: DetailRejection::Malformed,
            bytes: records[0].len(),
        }]
    );
}

/// No budget per run: 20 records of 200 KiB, four times what a run once
/// kept, are every one admissible.
#[test]
fn a_post_keeps_every_record_however_large_the_run() {
    let records: Vec<String> = (1..=20).map(|n| record(n, 200 * KIB)).collect();
    let body = post(&records);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (candidates, skipped) = narrow_all(&request.calls);
    assert!(skipped.is_empty(), "{skipped:?}");
    let (kept, unserved) = serving(candidates, &served(&[EVENT]));
    assert_eq!(kept.len(), 20);
    assert!(unserved.is_empty(), "{unserved:?}");
}

#[test]
fn a_skip_is_logged_by_position_and_size_never_content() {
    let log = Capture::install();
    log_skip(
        &target(7, 7),
        Skip {
            position: 2,
            reason: DetailRejection::Malformed,
            bytes: 70_000,
        },
    );
    let line = log.only(EVENT_SKIPPED);
    assert_eq!(line.level, tracing::Level::INFO);
    assert_eq!(line.field("reason"), Some("malformed"));
    assert_eq!(line.field("position"), Some("2"));
    assert_eq!(line.field("bytes"), Some("70000"));
    assert_eq!(line.field("agentsfleet_turn_id"), Some(EVENT));
}

#[test]
fn a_record_naming_a_turn_the_run_does_not_serve_is_skipped() {
    let records = [record(1, 4), of_turn(2, 4, OTHER), of_turn(3, 4, FOLDED)];
    let body = post(&records);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (candidates, skipped) = narrow_all(&request.calls);
    assert!(skipped.is_empty(), "{skipped:?}");

    let (kept, unserved) = serving(candidates, &served(&[EVENT, FOLDED]));

    assert_eq!(kept.keys().copied().collect::<Vec<_>>(), [1, 3]);
    assert_eq!(
        unserved,
        [Skip {
            position: 1,
            reason: DetailRejection::UnservedTurn,
            bytes: records[1].len(),
        }]
    );
}

#[test]
fn a_run_serving_none_of_the_named_turns_keeps_nothing() {
    let body = post(&[of_turn(1, 4, OTHER), of_turn(2, 4, FOLDED)]);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (candidates, _) = narrow_all(&request.calls);

    let (kept, unserved) = serving(candidates, &served(&[]));

    assert!(kept.is_empty());
    assert_eq!(
        unserved.iter().map(|skip| skip.reason).collect::<Vec<_>>(),
        [DetailRejection::UnservedTurn, DetailRejection::UnservedTurn]
    );
}

#[test]
fn the_turns_a_post_names_are_read_once_each() {
    let body = post(&[record(1, 4), of_turn(2, 4, FOLDED), record(3, 4)]);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (candidates, _) = narrow_all(&request.calls);

    let named: Vec<String> = named_turns(&candidates)
        .iter()
        .map(|turn| turn.as_str().to_owned())
        .collect();

    assert_eq!(named, [EVENT, FOLDED]);
}

#[test]
fn an_unserved_turn_skip_is_logged_under_its_reason() {
    let log = Capture::install();
    log_skip(
        &target(7, 7),
        Skip {
            position: 0,
            reason: DetailRejection::UnservedTurn,
            bytes: 256,
        },
    );
    assert_eq!(
        log.only(EVENT_SKIPPED).field("reason"),
        Some("unserved_turn")
    );
}
