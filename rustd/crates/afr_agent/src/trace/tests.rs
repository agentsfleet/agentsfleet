#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_wire::tool_trace::{
    ARGS_LEAF_MAX_BYTES, ARGS_MAX_BYTES, OUTPUT_EDGE_MAX_BYTES, TRACE_MAX_BYTES, TRACE_MAX_CALLS,
    ToolCallStatus, ToolTrace, ToolTraceCall, edge_fits,
};
use serde_json::{Map, json};

use super::{Outcome, Trace, bounded_arguments, encoded_len};

fn ended(output: &str) -> Outcome {
    Outcome::ended(
        ToolCallStatus::Succeeded,
        output,
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
fn should_replace_a_nul_the_store_cannot_hold() {
    let outcome = ended("bin\0ary");

    assert_eq!(outcome.head.as_deref(), Some("bin\u{fffd}ary"));
}

#[test]
fn should_carry_an_exit_code_only_when_one_was_given() {
    let ran = Outcome::ended(ToolCallStatus::Failed, "boom", Some(2), Duration::ZERO);

    assert_eq!(ran.exit_code, Some(2));
    assert_eq!(ended("ok").exit_code, None);
    assert_eq!(Outcome::interrupted(Duration::ZERO).line_count, None);
}

#[test]
fn should_cut_each_argument_to_its_leaf_bound() {
    let long = format!("{}é", "a".repeat(ARGS_LEAF_MAX_BYTES - 1));

    let bounded = bounded_arguments(&json!({"query": long, "nested": [{"deep": "x\0y"}]}));

    let query = bounded["query"].as_str().unwrap();
    assert_eq!(
        query.len(),
        ARGS_LEAF_MAX_BYTES - 1,
        "the split character is dropped whole"
    );
    assert_eq!(bounded["nested"][0]["deep"], "x\u{fffd}y");
}

#[test]
fn should_empty_arguments_that_stay_too_large_or_are_not_an_object() {
    let wide: Map<String, serde_json::Value> = (0..20)
        .map(|key| (format!("k{key}"), json!("v".repeat(200))))
        .collect();

    assert!(bounded_arguments(&serde_json::Value::Object(wide)).is_empty());
    assert!(bounded_arguments(&json!(["not", "an", "object"])).is_empty());
}

#[test]
fn should_keep_arguments_that_encode_to_exactly_their_bound() {
    let mut fields: Map<String, serde_json::Value> = (0..9)
        .map(|key| (format!("a{key}"), json!("x".repeat(200))))
        .collect();
    let room = ARGS_MAX_BYTES - encoded_len(&fields) - r#","z":"""#.len();
    fields.insert("z".to_owned(), json!("x".repeat(room)));
    assert_eq!(encoded_len(&fields), ARGS_MAX_BYTES);

    let kept = bounded_arguments(&serde_json::Value::Object(fields.clone()));
    fields.insert("z".to_owned(), json!("x".repeat(room + 1)));
    let emptied = bounded_arguments(&serde_json::Value::Object(fields));

    assert_eq!(kept.len(), 10);
    assert!(emptied.is_empty());
}

/// The row [`Trace::push`] builds for call `number` ending as `outcome`.
fn row(number: u64, outcome: &Outcome, edges: bool) -> ToolTraceCall<'static> {
    ToolTraceCall {
        call_id: number.to_string().into(),
        name: "calculator".into(),
        arguments: Map::new(),
        status: outcome.status,
        output_head: outcome.head.clone().filter(|_| edges).map(Into::into),
        output_tail: outcome.tail.clone().filter(|_| edges).map(Into::into),
        output_line_count: outcome.line_count,
        exit_code: outcome.exit_code,
        duration_ms: 3,
    }
}

#[test]
fn should_take_rows_that_exactly_fill_the_room() {
    let outcome = ended("four\n");
    let first = encoded_len(&row(1, &outcome, true));
    let second = encoded_len(&row(2, &outcome, true)) + ",".len();
    let mut trace = Trace {
        room: first + second,
        ..Trace::default()
    };

    trace.push(1, "calculator", Map::new(), &outcome);
    trace.push(2, "calculator", Map::new(), &outcome);

    assert_eq!(trace.room, 0);
    assert_eq!(trace.calls.len(), 2);
    assert!(trace.calls.iter().all(|call| call.output_head.is_some()));
}

#[test]
fn should_keep_a_later_row_without_edges_that_exactly_fills_the_room() {
    let outcome = ended("four\n");
    let first = encoded_len(&row(1, &outcome, true));
    let second = encoded_len(&row(2, &outcome, false)) + ",".len();
    let mut trace = Trace {
        room: first + second,
        ..Trace::default()
    };

    trace.push(1, "calculator", Map::new(), &outcome);
    trace.push(2, "calculator", Map::new(), &outcome);

    assert_eq!((trace.room, trace.calls.len()), (0, 2));
    assert!(trace.calls[0].output_head.is_some());
    assert!(
        trace.calls[1].output_head.is_none(),
        "the second row fit only without edges"
    );
}

#[test]
fn should_account_every_byte_the_rows_take() {
    let mut trace = Trace::default();
    for (number, output) in [(1, ""), (2, "one\n"), (3, "a\nb\nc\n"), (4, "x")] {
        trace.push(number, "calculator", Map::new(), &ended(output));
    }

    let listed = ToolTrace {
        calls: trace.calls.clone(),
        omitted_call_count: u64::MAX,
    };

    assert_eq!(encoded_len(&listed) + trace.room, TRACE_MAX_BYTES);
}

#[test]
fn test_trace_bounds_come_from_afd_wire() {
    let mut counted = Trace::default();
    for number in 1..=(TRACE_MAX_CALLS as u64 + 1) {
        counted.push(number, "calculator", Map::new(), &ended("4"));
    }
    let counted = counted.finish().unwrap();
    assert_eq!(counted.calls.len(), TRACE_MAX_CALLS);
    assert_eq!(counted.omitted_call_count, 1);
    assert_eq!(counted.validate(), Ok(()));

    // About 1 KiB of edges a call: 70 calls carry 70 KiB, past the 64 KiB cap.
    let edge = format!("{}\n", "x".repeat(199)).repeat(5);
    let mut sized = Trace::default();
    for number in 1..=70 {
        sized.push(number, "http_request", Map::new(), &ended(&edge));
    }
    let sized = sized.finish().unwrap();
    assert_eq!(sized.validate(), Ok(()));
    assert!(serde_json::to_vec(&sized).unwrap().len() <= TRACE_MAX_BYTES);
    assert!(sized.calls.first().unwrap().output_head.is_some());
    assert!(
        sized.calls.last().unwrap().output_head.is_none(),
        "later rows lost their edges"
    );
    assert_eq!(
        sized.calls.len() as u64 + sized.omitted_call_count,
        70,
        "every call is listed or counted"
    );
}

#[test]
fn should_finish_with_no_trace_for_a_run_that_called_nothing() {
    assert!(Trace::default().finish().is_none());
}
