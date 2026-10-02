#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and indexes what it built"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::tool_detail::{
    DETAIL_EVENT_MAX_BYTES, DETAIL_FIELD_MAX_BYTES, DetailRejection, ToolCallRecordsRequest,
};
use serde_json::json;

use super::{DetailTarget, EVENT_SKIPPED, Skip, log_skip, narrow_all, within_budget};

/// A post's body: `records` as given, under fence 7.
fn post(records: &[String]) -> String {
    format!(r#"{{"fencing_token":7,"calls":[{}]}}"#, records.join(","))
}

/// One record of `call_number` whose output is `output_bytes` long.
fn record(call_number: u64, output_bytes: usize) -> String {
    json!({"call_number": call_number, "arguments": {}, "truncated_arguments": false,
           "output": "a".repeat(output_bytes), "output_line_count": 1, "truncated": false})
    .to_string()
}

fn target(fence: i64, live_seq: i64) -> DetailTarget {
    DetailTarget {
        fleet_id: "01924f4e-0000-7000-8000-00000000fee7".to_owned(),
        workspace_id: "01924f4e-0000-7000-8000-000000000001".to_owned(),
        event_id: "1700000000000-0".to_owned(),
        fence,
        live_seq,
    }
}

#[test]
fn only_the_live_holder_of_its_own_fence_may_post() {
    assert!(target(7, 7).holds(7));
    assert!(!target(6, 7).holds(6), "a reclaim moved the fleet on");
    assert!(
        !target(7, 7).holds(6),
        "a token that is not the lease's own"
    );
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
        record(2, DETAIL_FIELD_MAX_BYTES + 1),
        record(3, 4),
    ]);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (kept, skipped) = narrow_all(&request.calls);
    assert_eq!(kept.keys().copied().collect::<Vec<_>>(), [1, 3]);
    assert_eq!(
        skipped
            .iter()
            .map(|skip| (skip.position, skip.reason))
            .collect::<Vec<_>>(),
        [
            (1, DetailRejection::Malformed),
            (2, DetailRejection::TooLarge)
        ]
    );
}

#[test]
fn a_call_named_twice_keeps_its_last_record() {
    let body = post(&[record(4, 1), record(4, 9)]);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (kept, skipped) = narrow_all(&request.calls);
    assert_eq!(kept[&4].position, 1);
    assert_eq!(
        skipped,
        [Skip {
            position: 0,
            reason: DetailRejection::Malformed,
            bytes: 3,
        }]
    );
}

#[test]
fn the_budget_keeps_the_lowest_calls_that_fit() {
    // `{}` arguments make each record exactly 64 KiB.
    let records: Vec<String> = (1..=20)
        .map(|n| record(n, DETAIL_FIELD_MAX_BYTES - 2))
        .collect();
    let body = post(&records);
    let request: ToolCallRecordsRequest<'_> = serde_json::from_str(&body).expect("a post");
    let (candidates, skipped) = narrow_all(&request.calls);
    assert!(skipped.is_empty());
    let (kept, over) = within_budget(candidates, 0);
    assert_eq!(kept.len(), 16, "16 × 64 KiB is the whole 1 MiB");
    assert_eq!(over.len(), 4);
    assert!(
        over.iter()
            .all(|skip| skip.reason == DetailRejection::OverBudget)
    );
    assert_eq!(kept.last().map(|kept| kept.record.call_number), Some(16));

    let (candidates, _) = narrow_all(&request.calls);
    let (kept, over) = within_budget(candidates, DETAIL_EVENT_MAX_BYTES);
    assert!(
        kept.is_empty(),
        "an event already at its budget keeps nothing more"
    );
    assert_eq!(over.len(), 20);
}

#[test]
fn a_skip_is_logged_by_position_and_size_never_content() {
    let log = Capture::install();
    log_skip(
        &target(7, 7),
        Skip {
            position: 2,
            reason: DetailRejection::TooLarge,
            bytes: 70_000,
        },
    );
    let line = log.only(EVENT_SKIPPED);
    assert_eq!(line.level, tracing::Level::INFO);
    assert_eq!(line.field("reason"), Some("too_large"));
    assert_eq!(line.field("position"), Some("2"));
    assert_eq!(line.field("bytes"), Some("70000"));
    assert_eq!(line.field("agentsfleet_event_id"), Some("1700000000000-0"));
}
