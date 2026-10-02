#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking, and indexes fixtures it built"
)]

use std::borrow::Cow;

use serde_json::{Map, Value, json};

use super::{
    ARGS_LEAF_MAX_BYTES, ARGS_MAX_BYTES, OUTPUT_EDGE_MAX_BYTES, OUTPUT_EDGE_MAX_LINES,
    TRACE_MAX_BYTES, TRACE_MAX_CALLS, ToolCallStatus, ToolTrace, ToolTraceCall, TraceRejection,
    edge_fits,
};
use crate::activity::CALL_ID_MAX_BYTES;

/// One call with small arguments and both edges, valid against every bound.
fn call(call_id: &str) -> ToolTraceCall<'static> {
    let mut arguments = Map::new();
    arguments.insert("path".to_owned(), json!("README.md"));
    ToolTraceCall {
        call_id: Cow::Owned(call_id.to_owned()),
        name: Cow::Borrowed("file_read"),
        arguments,
        status: ToolCallStatus::Succeeded,
        output_head: Some(Cow::Borrowed("# agentsfleet")),
        output_tail: Some(Cow::Borrowed("MIT")),
        output_line_count: Some(214),
        exit_code: None,
        duration_ms: 12,
    }
}

fn trace(calls: Vec<ToolTraceCall<'static>>) -> ToolTrace<'static> {
    ToolTrace {
        calls,
        omitted_call_count: 0,
    }
}

/// A call whose arguments hold one string of `bytes` bytes under `key`.
fn call_with_argument(key: &str, bytes: usize) -> ToolTraceCall<'static> {
    let mut built = call("1");
    built.arguments = Map::new();
    built
        .arguments
        .insert(key.to_owned(), json!("a".repeat(bytes)));
    built
}

#[test]
fn test_tool_trace_validator_enforces_bounds() {
    assert_eq!(trace(vec![call("1"), call("2")]).validate(), Ok(()));

    let full: Vec<_> = (1..=TRACE_MAX_CALLS)
        .map(|n| call(&n.to_string()))
        .collect();
    assert_eq!(trace(full.clone()).validate(), Ok(()), "exactly 200 fits");
    let mut over = full;
    over.push(call("201"));
    assert_eq!(
        trace(over).validate(),
        Err(TraceRejection::TooManyCalls),
        "201 calls"
    );

    // The leaf is over its own bound here, which the object bound beats.
    assert_eq!(
        trace(vec![call_with_argument("k", ARGS_MAX_BYTES - 7)]).validate(),
        Err(TraceRejection::ArgumentsTooLarge),
        "2049-byte arguments"
    );

    let mut long_edge = call("1");
    long_edge.output_tail = Some(Cow::Owned("a".repeat(OUTPUT_EDGE_MAX_BYTES + 1)));
    assert_eq!(
        trace(vec![long_edge]).validate(),
        Err(TraceRejection::EdgeTooLarge),
        "1025-byte edge"
    );
}

#[test]
fn a_trace_over_its_byte_bound_is_too_large() {
    // Leaves just under their own bound, so only the trace total breaks.
    let wide: Vec<_> = (1..=40)
        .map(|n| {
            let mut built = call(&n.to_string());
            built.arguments = (0..7)
                .map(|k| (format!("k{k}"), json!("a".repeat(ARGS_LEAF_MAX_BYTES))))
                .collect();
            built
        })
        .collect();
    let encoded = serde_json::to_vec(&trace(wide.clone()))
        .expect("encodes")
        .len();
    assert!(encoded > TRACE_MAX_BYTES, "fixture is over: {encoded}");
    assert_eq!(trace(wide).validate(), Err(TraceRejection::TooLarge));
}

#[test]
fn arguments_are_bounded_as_an_object_and_by_each_key_and_leaf() {
    // `{"k":"…"}` is 8 bytes of framing around the value.
    let at_cap = call_with_argument("k", ARGS_MAX_BYTES - 8);
    let encoded = serde_json::to_vec(&at_cap.arguments)
        .expect("encodes")
        .len();
    assert_eq!(encoded, ARGS_MAX_BYTES);
    assert_eq!(
        trace(vec![call_with_argument("k", ARGS_LEAF_MAX_BYTES)]).validate(),
        Ok(())
    );
    assert_eq!(
        trace(vec![call_with_argument("k", ARGS_LEAF_MAX_BYTES + 1)]).validate(),
        Err(TraceRejection::ArgumentTooLong)
    );
    let long_key = "k".repeat(ARGS_LEAF_MAX_BYTES + 1);
    let mut keyed = call("1");
    keyed.arguments.insert(long_key.clone(), json!(1));
    assert_eq!(
        trace(vec![keyed]).validate(),
        Err(TraceRejection::ArgumentTooLong),
        "a key is bounded like a leaf"
    );
    let mut nested = call("1");
    nested
        .arguments
        .insert("outer".to_owned(), json!({ long_key: 1 }));
    assert_eq!(
        trace(vec![nested]).validate(),
        Err(TraceRejection::ArgumentTooLong)
    );
}

#[test]
fn a_nul_anywhere_in_a_call_drops_the_trace() {
    let mut cases = Vec::new();
    let mut edge = call("1");
    edge.output_head = Some(Cow::Borrowed("bin\u{0}ary"));
    cases.push(edge);
    let mut tail = call("1");
    tail.output_tail = Some(Cow::Borrowed("\u{0}"));
    cases.push(tail);
    let mut named = call("1");
    named.name = Cow::Borrowed("sh\u{0}");
    cases.push(named);
    cases.push(call("1\u{0}"));
    for arguments in [json!("x\u{0}"), json!(["\u{0}"]), json!({"in\u{0}": 1})] {
        let mut argued = call("1");
        argued.arguments.insert("k".to_owned(), arguments);
        cases.push(argued);
    }
    let mut keyed = call("1");
    keyed.arguments.insert("k\u{0}".to_owned(), json!(1));
    cases.push(keyed);
    for case in cases {
        assert_eq!(
            trace(vec![case.clone()]).validate(),
            Err(TraceRejection::HoldsNul),
            "{case:?}"
        );
    }
}

#[test]
fn a_call_id_outside_its_bound_refuses_the_trace() {
    assert_eq!(
        trace(vec![call(&"c".repeat(CALL_ID_MAX_BYTES))]).validate(),
        Ok(())
    );
    for unusable in [String::new(), "c".repeat(CALL_ID_MAX_BYTES + 1)] {
        assert_eq!(
            trace(vec![call(&unusable)]).validate(),
            Err(TraceRejection::CallIdUnusable)
        );
    }
}

#[test]
fn nested_argument_strings_are_bounded_too() {
    let too_long = "a".repeat(ARGS_LEAF_MAX_BYTES + 1);
    for nested in [
        json!([1, true, null, too_long]),
        json!({"inner": {"deeper": [too_long]}}),
    ] {
        let mut built = call("1");
        built.arguments = Map::new();
        built.arguments.insert("outer".to_owned(), nested);
        assert_eq!(
            trace(vec![built]).validate(),
            Err(TraceRejection::ArgumentTooLong)
        );
    }
    let mut fine = call("1");
    fine.arguments
        .insert("flags".to_owned(), json!([1, 2.5, false, null, {"k": "v"}]));
    assert_eq!(trace(vec![fine]).validate(), Ok(()));
}

#[test]
fn an_edge_is_bounded_by_lines_as_well_as_bytes() {
    let five = "1\n2\n3\n4\n5";
    assert!(edge_fits(five));
    assert!(
        edge_fits(&format!("{five}\n")),
        "a trailing newline ends line 5"
    );
    assert!(!edge_fits(&format!("{five}\n6")));
    assert!(edge_fits(&"a".repeat(OUTPUT_EDGE_MAX_BYTES)));
    assert!(!edge_fits(&"a".repeat(OUTPUT_EDGE_MAX_BYTES + 1)));
    assert_eq!(OUTPUT_EDGE_MAX_LINES, five.lines().count());

    let mut head = call("1");
    head.output_head = Some(Cow::Owned(format!("{five}\n6")));
    assert_eq!(
        trace(vec![head]).validate(),
        Err(TraceRejection::EdgeTooLarge)
    );
}

#[test]
fn a_trace_round_trips_with_every_status_and_optional_field() {
    for status in [
        ToolCallStatus::Succeeded,
        ToolCallStatus::Failed,
        ToolCallStatus::Interrupted,
    ] {
        let mut built = call("7:3");
        built.status = status;
        built.exit_code = Some(2);
        let sent = ToolTrace {
            calls: vec![built],
            omitted_call_count: 4,
        };
        let text = serde_json::to_string(&sent).expect("encodes");
        let back: ToolTrace<'_> = serde_json::from_str(&text).expect("decodes");
        assert_eq!(back, sent);
    }

    let sent = json!({
        "calls": [{"call_id": "1", "name": "shell", "arguments": {}, "status": "interrupted",
                   "duration_ms": 0}],
        "omitted_call_count": 0,
    })
    .to_string();
    let bare: ToolTrace<'_> = serde_json::from_str(&sent).expect("optional fields may be absent");
    let first = &bare.calls[0];
    assert_eq!(first.output_head, None);
    assert_eq!(first.output_line_count, None);
    let encoded = serde_json::to_value(&bare).expect("encodes");
    assert_eq!(
        encoded["calls"][0].get("exit_code"),
        None,
        "an absent optional is omitted, not null"
    );

    let unknown = serde_json::from_str::<ToolTrace<'_>>(
        r#"{"calls": [], "omitted_call_count": 0, "extra": 1}"#,
    );
    assert!(unknown.is_err(), "unknown fields are refused");
}

#[test]
fn every_rejection_has_its_own_spelling() {
    let all = [
        TraceRejection::TooManyCalls,
        TraceRejection::TooLarge,
        TraceRejection::CallIdUnusable,
        TraceRejection::ArgumentsTooLarge,
        TraceRejection::ArgumentTooLong,
        TraceRejection::EdgeTooLarge,
        TraceRejection::Malformed,
        TraceRejection::HoldsNul,
    ];
    let mut spellings: Vec<_> = all.iter().map(|reason| reason.as_str()).collect();
    spellings.sort_unstable();
    spellings.dedup();
    assert_eq!(spellings.len(), all.len());
    assert!(
        spellings
            .iter()
            .all(|word| word.bytes().all(|b| b == b'_' || b.is_ascii_lowercase()))
    );
}

#[test]
fn statuses_use_their_snake_case_spelling() {
    let spelled: Vec<Value> = [
        ToolCallStatus::Succeeded,
        ToolCallStatus::Failed,
        ToolCallStatus::Interrupted,
    ]
    .iter()
    .map(|status| serde_json::to_value(status).expect("encodes"))
    .collect();
    assert_eq!(
        spelled,
        [json!("succeeded"), json!("failed"), json!("interrupted")]
    );
}

/// The raw carrier a report holds, narrowed the way the daemon narrows it.
fn raw_narrowed(text: &str) -> Result<usize, TraceRejection> {
    let raw: super::RawToolTrace<'_> = serde_json::from_str(text).expect("any JSON is carried");
    raw.narrow().map(|kept| kept.calls.len())
}

#[test]
fn a_raw_trace_narrows_only_when_it_is_one_and_fits() {
    let kept = serde_json::to_string(&trace(vec![call("1"), call("2")])).expect("encodes");
    assert_eq!(raw_narrowed(&kept), Ok(2));
    assert_eq!(raw_narrowed("7"), Err(TraceRejection::Malformed));
    assert_eq!(
        raw_narrowed(r#"{"calls":[{"call_id":"1"}],"omitted_call_count":0}"#),
        Err(TraceRejection::Malformed)
    );
    // Inside the object: whitespace after a value is not part of it.
    let padded = format!("{{{}{}", " ".repeat(TRACE_MAX_BYTES), &kept[1..]);
    assert_eq!(
        raw_narrowed(&padded),
        Err(TraceRejection::TooLarge),
        "the bytes sent are bounded before any parse"
    );
    let unusable = serde_json::to_string(&trace(vec![call("")])).expect("encodes");
    assert_eq!(raw_narrowed(&unusable), Err(TraceRejection::CallIdUnusable));
}

#[test]
fn a_raw_trace_compares_by_its_text_and_reports_its_size() {
    let first: super::RawToolTrace<'_> = serde_json::from_str("[1]").expect("carried");
    let same: super::RawToolTrace<'_> = serde_json::from_str("[1]").expect("carried");
    let other: super::RawToolTrace<'_> = serde_json::from_str("[2]").expect("carried");
    assert_eq!(first, same);
    assert_ne!(first, other);
    assert_eq!(first.byte_len(), 3);
    assert_eq!(serde_json::to_string(&first).ok().as_deref(), Some("[1]"));
}

/// Every published bound on the trace is the constant that enforces it.
#[test]
fn published_descriptions_state_the_bounds_they_enforce() {
    use super::{CALL_ID_MAX_BYTES, OUTPUT_EDGE_MAX_LINES};
    let openapi = include_str!("../../../../../public/openapi.json");
    let document: serde_json::Value = serde_json::from_str(openapi).expect("the spec parses");
    let field = |schema: &str, property: &str| {
        document
            .pointer(&format!(
                "/components/schemas/{schema}/properties/{property}/description"
            ))
            .and_then(Value::as_str)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    let cases: [(&str, &str, &[usize]); 5] = [
        ("ToolTraceCall", "call_id", &[CALL_ID_MAX_BYTES]),
        (
            "ToolTraceCall",
            "arguments",
            &[ARGS_MAX_BYTES, ARGS_LEAF_MAX_BYTES],
        ),
        (
            "ToolTraceCall",
            "output_head",
            &[OUTPUT_EDGE_MAX_LINES, OUTPUT_EDGE_MAX_BYTES],
        ),
        (
            "ToolTraceCall",
            "output_tail",
            &[OUTPUT_EDGE_MAX_LINES, OUTPUT_EDGE_MAX_BYTES],
        ),
        ("ToolTrace", "calls", &[TRACE_MAX_CALLS]),
    ];
    for (schema, property, bounds) in cases {
        let text = field(schema, property);
        for bound in bounds {
            assert!(
                text.contains(&bound.to_string()),
                "{schema}.{property} lacks {bound}: {text}"
            );
        }
    }
}
