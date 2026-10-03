#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and indexes the post it built"
)]

use serde_json::json;

use super::{
    DETAIL_FIELD_MAX_BYTES, DETAIL_RECORD_MIN_BYTES, DetailRejection, RawToolCallRecord,
    ToolCallDetail, ToolCallRecord, ToolCallRecordsRequest, ToolCallRecordsStored,
};

/// A record as a runner posts it, with `output` and an empty argument object.
fn record(call_number: u64, output: &str) -> String {
    json!({"call_number": call_number, "arguments": {}, "truncated_arguments": false,
           "output": output, "output_line_count": 1, "truncated": false})
    .to_string()
}

/// What narrowing `sent` keeps: its byte count, or why it kept nothing.
fn narrowed(sent: &str) -> Result<usize, DetailRejection> {
    let raw: RawToolCallRecord<'_> = serde_json::from_str(sent).expect("any JSON is carried");
    raw.narrow().map(|kept| kept.byte_count())
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
    let sent = json!({"call_number": 1, "arguments": arguments, "truncated_arguments": false,
                      "output": "", "output_line_count": 0, "truncated": false});
    assert_eq!(
        narrowed(&sent.to_string()),
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
    ]
    .iter()
    .map(|reason| reason.as_str())
    .collect();
    assert_eq!(spelled, ["malformed", "too_large", "over_budget"]);
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
        arguments: serde_json::Map::new(),
        truncated_arguments: false,
        output: "all of it".into(),
        output_line_count: 224,
        truncated: false,
    };
    let text = serde_json::to_string(&detail).expect("encodes");
    let back: ToolCallDetail<'_> = serde_json::from_str(&text).expect("decodes");
    assert_eq!(back, detail);

    let sent = record(2, "x");
    let typed: ToolCallRecord<'_> = serde_json::from_str(&sent).expect("a record decodes typed");
    assert_eq!(typed.validate(), Ok(()));
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
        let sent = json!({"call_number": 1, "arguments": arguments, "truncated_arguments": false,
                          "output": "", "output_line_count": 0, "truncated": false});
        assert_eq!(
            narrowed(&sent.to_string()),
            Err(DetailRejection::Malformed),
            "{sent}"
        );
    }
}

/// Every published byte figure on this surface is the constant that enforces
/// it, so a bound cannot move without its description failing here.
#[test]
fn published_descriptions_state_the_bounds_they_enforce() {
    use super::DETAIL_POST_MAX_BYTES;
    let openapi = include_str!("../../../../../public/openapi.json");
    let document: serde_json::Value = serde_json::from_str(openapi).expect("the spec parses");
    let text = |pointer: &str| {
        document
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    let post = text("/paths/~1v1~1runners~1me~1leases~1{lease_id}~1tool-calls/post/description");
    for bound in [DETAIL_POST_MAX_BYTES, DETAIL_FIELD_MAX_BYTES] {
        assert!(post.contains(&bound.to_string()), "{bound}: {post}");
    }
    let read = text(
        "/paths/~1v1~1workspaces~1{workspace_id}~1fleets~1{fleet_id}~1events~1{event_id}~1tool-calls~1{call_id}/get/description",
    );
    assert!(read.contains(&DETAIL_FIELD_MAX_BYTES.to_string()), "{read}");
    for property in ["arguments", "output"] {
        let field = text(&format!(
            "/components/schemas/ToolCallRecord/properties/{property}/description"
        ));
        assert!(
            field.contains(&DETAIL_FIELD_MAX_BYTES.to_string()),
            "{property}: {field}"
        );
    }
}
