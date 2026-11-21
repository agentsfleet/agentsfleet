#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and indexes the post it built"
)]

use garde::Validate as _;
use serde_json::{Value, json};

use super::{
    DETAIL_FIELD_MAX_BYTES, DETAIL_RECORD_MIN_BYTES, DetailRejection, ItemKind, ItemStatus,
    RawToolCallRecord, ToolCallDetail, ToolCallRecord, ToolCallRecordsRequest,
    ToolCallRecordsStored,
};
use crate::tool_trace::FileChange;

/// A turn id as a runner names one.
const TURN: &str = "019f0000-0000-7000-8000-000000000001";

/// A record as a runner posts it: a `file_read` call of the run's turn,
/// with `output` and an empty argument object.
fn fields(call_number: u64, output: &str) -> Value {
    json!({"call_number": call_number, "arguments": {}, "truncated_arguments": false,
           "output": output, "output_line_count": 1, "truncated": false,
           "turn_id": TURN, "kind": "tool_call", "tool": "file_read", "status": "succeeded"})
}

/// [`fields`], encoded.
fn record(call_number: u64, output: &str) -> String {
    fields(call_number, output).to_string()
}

/// Call 1's record with `key` set to `value`.
fn with(key: &str, value: Value) -> String {
    let mut sent = fields(1, "x");
    sent[key] = value;
    sent.to_string()
}

/// Call 1's record without `key`.
fn without(key: &str) -> String {
    let mut sent = fields(1, "x");
    if let Some(fields) = sent.as_object_mut() {
        fields.remove(key);
    }
    sent.to_string()
}

/// An edit's record changing `changes`.
fn edit(changes: Value) -> String {
    let mut sent = fields(1, "Wrote 4 bytes to src/a.rs");
    sent["kind"] = json!("edit");
    sent["tool"] = json!("file_write");
    sent["changes"] = changes;
    sent.to_string()
}

/// What narrowing `sent` keeps: its byte count, or why it kept nothing.
fn narrowed(sent: &str) -> Result<usize, DetailRejection> {
    carried(sent).narrow().map(|kept| kept.byte_count())
}

/// The raw record `sent` reads as; any JSON is carried.
fn carried(sent: &str) -> RawToolCallRecord<'_> {
    serde_json::from_str(sent).expect("any JSON is carried")
}

#[test]
fn a_record_within_its_bounds_is_kept_and_counted() {
    assert_eq!(
        narrowed(&record(3, "hello")),
        Ok(DETAIL_RECORD_MIN_BYTES),
        "a tiny record still spends the per-row minimum"
    );
    // `{}` is two bytes of arguments beside the output.
    let at_cap = "a".repeat(DETAIL_FIELD_MAX_BYTES);
    assert_eq!(
        narrowed(&record(1, &at_cap)),
        Ok(DETAIL_FIELD_MAX_BYTES + 2)
    );
}

#[test]
fn a_record_past_a_field_bound_is_too_large() {
    let over = "a".repeat(DETAIL_FIELD_MAX_BYTES + 1);
    assert_eq!(narrowed(&record(1, &over)), Err(DetailRejection::TooLarge));
    let arguments = json!({"blob": "a".repeat(DETAIL_FIELD_MAX_BYTES)});
    assert_eq!(
        narrowed(&with("arguments", arguments)),
        Err(DetailRejection::TooLarge),
        "the arguments are bounded too"
    );
}

#[test]
fn a_record_of_the_wrong_shape_or_number_is_malformed() {
    assert_eq!(narrowed("7"), Err(DetailRejection::Malformed));
    assert_eq!(
        narrowed(r#"{"call_number": 1}"#),
        Err(DetailRejection::Malformed)
    );
    assert_eq!(
        narrowed(&record(0, "x")),
        Err(DetailRejection::Malformed),
        "calls count from 1"
    );
    assert_eq!(
        narrowed(&record(u64::MAX, "x")),
        Err(DetailRejection::Malformed),
        "a number the column cannot hold"
    );
}

#[test]
fn a_post_carries_each_record_unread_until_it_is_narrowed() {
    let body = format!(r#"{{"fencing_token":7,"calls":[{},7]}}"#, record(1, "kept"));
    let request: ToolCallRecordsRequest<'_> =
        serde_json::from_str(&body).expect("one bad record does not refuse the post");
    assert_eq!(request.fencing_token, 7);
    let kept: Vec<_> = request
        .calls
        .iter()
        .map(|raw| raw.narrow().is_ok())
        .collect();
    assert_eq!(kept, [true, false]);
    assert_eq!(request.calls[0].byte_len(), record(1, "kept").len());
    assert_ne!(request.calls[0], request.calls[1]);
    assert_eq!(request.calls[0], request.calls[0]);
    assert_eq!(
        serde_json::to_string(&request).expect("encodes"),
        body,
        "a carried record re-encodes as it was sent"
    );
}

#[test]
fn every_rejection_has_its_own_spelling() {
    let spelled: Vec<_> = [
        DetailRejection::Malformed,
        DetailRejection::TooLarge,
        DetailRejection::OverBudget,
        DetailRejection::UnservedTurn,
    ]
    .iter()
    .map(|reason| reason.as_str())
    .collect();
    assert_eq!(
        spelled,
        ["malformed", "too_large", "over_budget", "unserved_turn"]
    );
}

#[test]
fn the_answers_round_trip() {
    let stored = ToolCallRecordsStored {
        stored_count: 2,
        skipped_count: 1,
    };
    let text = serde_json::to_string(&stored).expect("encodes");
    assert_eq!(text, r#"{"stored_count":2,"skipped_count":1}"#);

    let detail = ToolCallDetail {
        call_id: "7:3".into(),
        kind: ItemKind::Edit,
        tool: Some("file_edit".into()),
        status: ItemStatus::Succeeded,
        arguments: serde_json::Map::new(),
        truncated_arguments: false,
        output: "all of it".into(),
        output_line_count: 224,
        truncated: false,
        changes: vec![FileChange {
            path: "src/a.rs".into(),
            added: 24,
            removed: 8,
        }],
    };
    let text = serde_json::to_string(&detail).expect("encodes");
    let back: ToolCallDetail<'_> = serde_json::from_str(&text).expect("decodes");
    assert_eq!(back, detail);

    let sent = record(2, "x");
    let typed: ToolCallRecord<'_> = serde_json::from_str(&sent).expect("a record decodes typed");
    typed
        .validate()
        .expect("a well-formed record meets every bound");
}

/// A drop or a skip is logged under the reason it was logged under before
/// the bounds moved onto the types, so an operator's query still finds it.
#[test]
fn test_trace_and_detail_rejections_keep_their_reasons() {
    use crate::tool_trace::{RawToolTrace, TRACE_MAX_CALLS, TraceRejection};

    let calls: Vec<_> = (1..=TRACE_MAX_CALLS + 1)
        .map(|number| {
            json!({"call_id": number.to_string(), "name": "file_read", "arguments": {},
                   "status": "succeeded", "duration_ms": 1})
        })
        .collect();
    let sent = json!({"calls": calls, "omitted_call_count": 0}).to_string();
    let raw: RawToolTrace<'_> = serde_json::from_str(&sent).expect("any JSON is carried");
    assert_eq!(
        raw.narrow().map(|_kept| ()).map_err(TraceRejection::as_str),
        Err("too_many_calls")
    );

    let reason = |sent: &str| narrowed(sent).map_err(DetailRejection::as_str);
    let over = "a".repeat(DETAIL_FIELD_MAX_BYTES + 1);
    assert_eq!(reason(&record(1, &over)), Err("too_large"));
    assert_eq!(reason(&record(0, "x")), Err("malformed"));
}

#[test]
fn a_record_holding_a_nul_is_skipped_not_fatal() {
    assert_eq!(
        narrowed(&record(1, "a\u{0}b")),
        Err(DetailRejection::Malformed)
    );
    for arguments in [
        json!({"k": "x\u{0}"}),
        json!({"k\u{0}": 1}),
        json!({"k": ["\u{0}"]}),
    ] {
        let sent = with("arguments", arguments);
        assert_eq!(narrowed(&sent), Err(DetailRejection::Malformed), "{sent}");
    }
    assert_eq!(
        narrowed(&with("tool", json!("file\u{0}read"))),
        Err(DetailRejection::Malformed),
        "the tool's name is a stored string too"
    );
    assert_eq!(
        narrowed(&edit(
            json!([{"path": "a\u{0}.rs", "added": 1, "removed": 0}])
        )),
        Err(DetailRejection::Malformed),
        "so is a changed file's path"
    );
}

#[test]
fn an_edit_record_keeps_each_files_counts() {
    let sent = edit(json!([{"path": "src/a.rs", "added": 24, "removed": 8},
                           {"path": "docs/naïve.md", "added": 0, "removed": 3}]));
    let kept = carried(&sent).narrow().expect("an edit within its bounds");
    assert_eq!(kept.kind, ItemKind::Edit);
    assert_eq!(kept.tool, "file_write");
    assert_eq!(kept.status, ItemStatus::Succeeded);
    assert_eq!(kept.turn_id, TURN);
    assert_eq!(
        kept.changes,
        [
            FileChange {
                path: "src/a.rs".into(),
                added: 24,
                removed: 8
            },
            FileChange {
                path: "docs/naïve.md".into(),
                added: 0,
                removed: 3
            },
        ]
    );
    let text = serde_json::to_string(&kept).expect("encodes");
    assert!(
        !serde_json::to_string(&carried(&record(1, "x")).narrow().expect("kept"))
            .expect("encodes")
            .contains("changes"),
        "a call that changed nothing carries no changes field"
    );
    assert!(text.contains(r#""changes":[{"path":"src/a.rs","added":24,"removed":8}"#));
}

#[test]
fn a_record_naming_no_turn_or_an_unusable_one_is_malformed() {
    for sent in [
        without("turn_id"),
        with("turn_id", json!("")),
        with("turn_id", json!("not-a-turn")),
        with("turn_id", json!(TURN.to_uppercase())),
        // A version 4 UUID: not one this system mints.
        with("turn_id", json!("019f0000-0000-4000-8000-000000000001")),
        with("turn_id", json!(format!("{TURN}0"))),
    ] {
        assert_eq!(narrowed(&sent), Err(DetailRejection::Malformed), "{sent}");
    }
}

#[test]
fn a_record_without_its_kind_tool_or_status_is_malformed() {
    for sent in [
        without("kind"),
        with("kind", json!("approval")),
        without("tool"),
        with("tool", json!("")),
        without("status"),
        with("status", json!("interrupted")),
        with("status", json!("running")),
    ] {
        assert_eq!(narrowed(&sent), Err(DetailRejection::Malformed), "{sent}");
    }
    assert!(
        narrowed(&with("tool", json!("t".repeat(4096)))).is_ok_and(|bytes| bytes > 0),
        "no length caps a tool name"
    );
}

#[test]
fn a_malformed_change_is_malformed_and_no_count_caps_them() {
    let one = |path: String| json!({"path": path, "added": 1, "removed": 1});
    for changes in [
        json!([one(String::new())]),
        json!([{"path": "a.rs", "added": -1, "removed": 0}]),
        json!([{"path": "a.rs", "added": 1}]),
        json!([{"path": "a.rs", "added": 1, "removed": 0, "renamed": true}]),
        json!({"path": "a.rs", "added": 1, "removed": 0}),
    ] {
        let sent = edit(changes);
        assert_eq!(narrowed(&sent), Err(DetailRejection::Malformed), "{sent}");
    }
    let many: Vec<Value> = (1..=100).map(|file| one(format!("f{file}"))).collect();
    assert!(
        narrowed(&edit(json!(many))).is_ok_and(|bytes| bytes > 0),
        "100 files are kept"
    );
    assert!(
        narrowed(&edit(json!([one("p".repeat(4096))]))).is_ok_and(|bytes| bytes > 0),
        "no length caps a path"
    );
}
