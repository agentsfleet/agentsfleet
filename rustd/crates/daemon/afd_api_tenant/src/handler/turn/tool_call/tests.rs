#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and indexes the JSON it built"
)]

use afd_turns::view::{CallAddress, ToolCallRow};

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
    assert_eq!(answered["kind"], "tool_call");
    assert_eq!(answered["tool"], "shell");
    assert_eq!(answered["status"], "succeeded");
}

#[test]
fn an_edits_record_answers_with_each_files_counts() {
    let row = ToolCallRow::fixture("Success.")
        .with_changes(r#"[{"path":"src/a.rs","added":24,"removed":8}]"#);
    let call = parse_call_id("7:1").expect("a call id");
    let answered = serde_json::to_value(detail(call, &row)).expect("encodes");
    assert_eq!(
        answered["changes"],
        serde_json::json!([{"path": "src/a.rs", "added": 24, "removed": 8}])
    );
    assert_eq!(answered["kind"], "edit");
    assert_eq!(answered["tool"], "file_edit");
    assert_eq!(answered["status"], "succeeded");
}

#[test]
fn a_record_that_changed_nothing_answers_no_changes() {
    let call = parse_call_id("7:1").expect("a call id");
    for row in [
        ToolCallRow::fixture("ran"),
        // A column this build cannot read answers as no changes, never a 500.
        ToolCallRow::fixture("ran").with_changes("not a list"),
    ] {
        let answered = serde_json::to_value(detail(call, &row)).expect("encodes");
        assert!(answered.get("changes").is_none(), "{answered}");
    }
}
