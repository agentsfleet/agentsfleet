#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use af_wire::tool_trace::{
    ARGS_LEAF_MAX_BYTES, ARGS_MAX_BYTES, OUTPUT_EDGE_MAX_BYTES, ToolCallStatus, edge_fits,
};
use serde_json::{Map, json};

use super::{Outcome, Trace, bounded_arguments, encoded_len};
use crate::fixture::{clean, clean_json};

fn ended(output: &str) -> Outcome {
    Outcome::ended(
        ToolCallStatus::Succeeded,
        &clean(output),
        None,
        Duration::from_millis(3),
    )
}

fn numbered(lines: usize) -> String {
    (1..=lines)
        .map(|line| format!("line {line}\n"))
        .collect::<Vec<_>>()
        .concat()
}

#[test]
fn should_keep_a_short_output_whole_in_the_head() {
    let outcome = ended("one\ntwo\n");

    assert_eq!(outcome.head.as_deref(), Some("one\ntwo\n"));
    assert_eq!(outcome.tail, None);
    assert_eq!(outcome.line_count, Some(2));
}

#[test]
fn should_split_a_long_output_into_five_lines_each_end() {
    let outcome = ended(&numbered(12));

    assert_eq!(outcome.head.as_deref(), Some(numbered(5).as_str()));
    let tail = outcome.tail.unwrap();
    assert_eq!(tail, "line 8\nline 9\nline 10\nline 11\nline 12\n");
    assert_eq!(outcome.line_count, Some(12));
}

#[test]
fn should_cut_one_long_line_on_a_character_boundary() {
    let output = "é".repeat(OUTPUT_EDGE_MAX_BYTES);

    let outcome = ended(&output);

    let (head, tail) = (outcome.head.unwrap(), outcome.tail.unwrap());
    assert!(edge_fits(&head) && edge_fits(&tail));
    assert!(head.len() > OUTPUT_EDGE_MAX_BYTES - "é".len() && tail.len() == OUTPUT_EDGE_MAX_BYTES);
    assert_eq!(outcome.line_count, Some(1));
}

#[test]
fn should_carry_no_edges_for_an_empty_output() {
    let outcome = ended("");

    assert_eq!(
        (outcome.head, outcome.tail, outcome.line_count),
        (None, None, Some(0))
    );
}

#[test]
fn should_carry_an_exit_code_only_when_one_was_given() {
    let ran = Outcome::ended(
        ToolCallStatus::Failed,
        &clean("boom"),
        Some(2),
        Duration::ZERO,
    );

    assert_eq!(ran.exit_code, Some(2));
    assert_eq!(ended("ok").exit_code, None);
    assert_eq!(Outcome::interrupted(Duration::ZERO).line_count, None);
}

#[test]
fn should_cut_each_argument_to_its_leaf_bound() {
    let long = format!("{}é", "a".repeat(ARGS_LEAF_MAX_BYTES - 1));

    let bounded = bounded_arguments(&clean_json(json!({"query": long, "nested": [{long: "y"}]})));

    let query = bounded["query"].as_str().unwrap();
    assert_eq!(
        query.len(),
        ARGS_LEAF_MAX_BYTES - 1,
        "the split character is dropped whole"
    );
    let nested = bounded["nested"][0].as_object().unwrap();
    assert!(
        nested.keys().all(|key| key.len() < ARGS_LEAF_MAX_BYTES),
        "a key is cut like a string"
    );
}

#[test]
fn should_empty_arguments_that_stay_too_large_or_are_not_an_object() {
    let wide: Map<String, serde_json::Value> = (0..20)
        .map(|key| (format!("k{key}"), json!("v".repeat(200))))
        .collect();

    assert!(bounded_arguments(&clean_json(serde_json::Value::Object(wide))).is_empty());
    assert!(bounded_arguments(&clean_json(json!(["not", "an", "object"]))).is_empty());
}

#[test]
fn should_keep_arguments_that_encode_to_exactly_their_bound() {
    let mut fields: Map<String, serde_json::Value> = (0..9)
        .map(|key| (format!("a{key}"), json!("x".repeat(200))))
        .collect();
    let room = ARGS_MAX_BYTES - encoded_len(&fields) - r#","z":"""#.len();
    fields.insert("z".to_owned(), json!("x".repeat(room)));
    assert_eq!(encoded_len(&fields), ARGS_MAX_BYTES);

    let kept = bounded_arguments(&clean_json(serde_json::Value::Object(fields.clone())));
    fields.insert("z".to_owned(), json!("x".repeat(room + 1)));
    let emptied = bounded_arguments(&clean_json(serde_json::Value::Object(fields)));

    assert_eq!(kept.len(), 10);
    assert!(emptied.is_empty());
}

/// No count or size caps the trace: 300 calls, each with about 2 KiB of
/// edges, are every one listed with its preview, past the 200 calls and
/// 64 KiB a trace was once held to.
#[test]
fn every_call_is_listed_with_its_preview() {
    let edge = format!("{}\n", "x".repeat(199)).repeat(10);
    let mut trace = Trace::default();
    for number in 1..=300 {
        let arguments = serde_json::from_value(json!({
            "url": format!("https://example.test/jobs/{number}/logs"),
        }))
        .unwrap();
        trace.push(number, "http_request", arguments, ended(&edge));
    }
    let trace = trace.finish();

    assert_eq!(trace.validate(), Ok(()));
    assert_eq!(trace.calls.len(), 300);
    assert!(encoded_len(&trace) > 64 * 1024);
    for (index, call) in trace.calls.iter().enumerate() {
        assert_eq!(call.call_id, (index + 1).to_string());
        assert!(call.output_head.is_some() && call.output_tail.is_some());
    }
}

#[test]
fn should_finish_with_a_known_empty_trace_when_no_tool_ran() {
    assert_eq!(json!(Trace::default().finish()), json!({"calls": []}));
}
