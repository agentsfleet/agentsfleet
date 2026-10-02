#![expect(clippy::expect_used, reason = "a test asserts by panicking")]

use afd_events::{CallAddress, ToolCallRow};

use super::{detail, parse_call_id};

#[test]
fn a_fenced_call_id_names_its_fence_and_number() {
    assert_eq!(
        parse_call_id("7:3"),
        Some(CallAddress {
            fence: 7,
            call_number: 3,
        })
    );
    assert_eq!(parse_call_id("x:y:z"), None);
}

#[test]
fn a_kept_record_answers_in_full_under_its_canonical_id() {
    let row = ToolCallRow::fixture("line one\nline two");
    let call = parse_call_id("07:3").expect("a leading zero still names fence 7");
    let answered = serde_json::to_value(detail(call, &row)).expect("encodes");
    assert_eq!(answered["call_id"], "7:3");
    assert_eq!(answered["output"], "line one\nline two");
    assert_eq!(answered["output_line_count"], 2);
    assert_eq!(answered["arguments"], serde_json::json!({}));
    assert_eq!(answered["truncated"], false);
    assert_eq!(answered["truncated_arguments"], false);
}
