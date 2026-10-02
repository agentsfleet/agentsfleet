#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and indexes the JSON it built"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::{RawToolTrace, TRACE_MAX_CALLS};
use serde_json::{Value, json};

use super::{EVENT_TRACE_DROPPED, TraceOwner, stored};

/// The fence the fixture lease holds.
const FENCE: i64 = 7;

const OWNER: TraceOwner<'static> = TraceOwner {
    fleet_id: "01924f4e-0000-7000-8000-00000000fee7",
    event_id: "1700000000000-0",
    fence: FENCE,
};

/// One call the way a runner sends it, numbered `n`.
fn call(n: usize) -> Value {
    json!({"call_id": n.to_string(), "name": "file_read", "arguments": {"path": "README.md"},
           "status": "succeeded", "output_head": "# agentsfleet", "output_line_count": 214,
           "duration_ms": 12})
}

/// What the event row would store for `sent`.
fn stored_from(sent: &str) -> Option<Value> {
    let raw: RawToolTrace<'_> = serde_json::from_str(sent).expect("any JSON is carried");
    stored(Some(raw), OWNER).map(|text| serde_json::from_str(&text).expect("stored JSON"))
}

#[test]
fn a_stored_trace_carries_fenced_call_ids_and_nothing_else_changes() {
    let sent = json!({"calls": [call(1), call(2)], "omitted_call_count": 3});
    let kept = stored_from(&sent.to_string()).expect("a valid trace is kept");
    assert_eq!(kept["calls"][0]["call_id"], json!("7:1"));
    assert_eq!(kept["calls"][1]["call_id"], json!("7:2"));
    let mut unfenced = kept.clone();
    for (n, call) in unfenced["calls"]
        .as_array_mut()
        .expect("calls")
        .iter_mut()
        .enumerate()
    {
        call["call_id"] = json!((n + 1).to_string());
    }
    assert_eq!(unfenced, sent, "only the call ids are rewritten");
}

#[test]
fn no_trace_sent_stores_none_and_logs_nothing() {
    let log = Capture::install();
    assert_eq!(stored(None, OWNER), None);
    assert!(log.events().is_empty(), "{:?}", log.events());
}

#[test]
fn a_trace_over_a_bound_is_dropped_with_a_log_line_carrying_no_content() {
    let log = Capture::install();
    let calls: Vec<Value> = (1..=TRACE_MAX_CALLS + 1).map(call).collect();
    let sent = json!({"calls": calls, "omitted_call_count": 0}).to_string();
    assert_eq!(stored_from(&sent), None);
    let line = log.only(EVENT_TRACE_DROPPED);
    assert_eq!(line.level, tracing::Level::WARN);
    assert_eq!(line.field("reason"), Some("too_many_calls"));
    assert_eq!(line.field("fleet_id"), Some(OWNER.fleet_id));
    assert_eq!(line.field("agentsfleet_event_id"), Some(OWNER.event_id));
    assert_eq!(line.field("bytes"), Some(sent.len().to_string().as_str()));
    assert!(
        line.fields.values().all(|value| !value.contains("README")),
        "the trace body is never logged: {:?}",
        line.fields
    );
}

#[test]
fn a_trace_of_the_wrong_shape_is_dropped_as_malformed() {
    let log = Capture::install();
    assert_eq!(stored_from("7"), None);
    assert_eq!(
        log.only(EVENT_TRACE_DROPPED).field("reason"),
        Some("malformed")
    );
}
