#![expect(
    clippy::panic,
    reason = "a test asserts by panicking, naming the body that refused"
)]

use super::Outcome;

#[test]
fn runner_outcomes_use_the_same_spelling_in_rows_and_json() {
    for (outcome, stored) in [
        (Outcome::Processed, "processed"),
        (Outcome::FleetError, "fleet_error"),
    ] {
        assert_eq!(outcome.as_str(), stored);
        let encoded = serde_json::to_value(outcome);
        assert_eq!(
            encoded.as_ref().ok().and_then(serde_json::Value::as_str),
            Some(stored)
        );
    }
}

/// A report a runner sends, with `tool_calls` spliced in as given.
fn report_with(tool_calls: &str) -> String {
    format!(
        r#"{{"lease_id":"lease","event_id":"1-0","fencing_token":7,"outcome":"processed",
            "failure_reason":null,"failure_detail":"","response_text":"done","tokens":3,
            "input_tokens":1,"cached_input_tokens":0,"output_tokens":2,
            "telemetry":{{"time_to_first_token_ms":4,"wall_ms":5}},
            "checkpoint":{{"last_event_id":"1-0","last_response":""}}{tool_calls}}}"#
    )
}

/// What the daemon keeps of the report's trace, or why it kept none.
fn kept(body: &str) -> Option<Result<usize, crate::tool_trace::TraceRejection>> {
    let request: super::ReportRequest<'_> =
        serde_json::from_str(body).unwrap_or_else(|refused| panic!("{refused}: {body}"));
    request
        .tool_calls
        .map(|raw| raw.narrow().map(|trace| trace.calls.len()))
}

#[test]
fn test_report_tool_calls_never_refuse_report() {
    use crate::tool_trace::TraceRejection;
    assert_eq!(kept(&report_with("")), None, "absent");
    assert_eq!(kept(&report_with(r#","tool_calls":null"#)), None, "null");
    assert_eq!(
        kept(&report_with(r#","tool_calls":7"#)),
        Some(Err(TraceRejection::Malformed)),
        "the wrong shape parses with the report and is dropped on its own"
    );
    assert_eq!(
        kept(&report_with(
            r#","tool_calls":{"calls":[],"omitted_call_count":0,"x":1}"#
        )),
        Some(Err(TraceRejection::Malformed))
    );
    let valid = r#","tool_calls":{"calls":[{"call_id":"3","name":"file_read",
        "arguments":{"path":"README.md"},"status":"succeeded","duration_ms":12}],
        "omitted_call_count":0}"#;
    assert_eq!(
        kept(&report_with(valid)),
        Some(Ok(1)),
        "only a valid trace survives"
    );
}
