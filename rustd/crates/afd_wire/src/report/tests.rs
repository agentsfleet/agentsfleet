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
            "requests":[{{"seq":0,"at":"2026-10-19T10:00:00Z","input_tokens":1,
                "cached_input_tokens":0,"cache_write_tokens":0,"cache_write_5m_tokens":0,
                "cache_write_1h_tokens":0,"output_tokens":2}}],
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

/// A runner still sending the cumulative totals the daemon used to diff is
/// refused, naming the field: per-request lines replaced them, and no alias
/// keeps the old body alive.
#[test]
fn a_cumulative_report_body_is_refused() {
    let old = report_with("").replace(
        r#""requests":[{"seq":0,"at":"2026-10-19T10:00:00Z","input_tokens":1,
                "cached_input_tokens":0,"cache_write_tokens":0,"cache_write_5m_tokens":0,
                "cache_write_1h_tokens":0,"output_tokens":2}],"#,
        r#""requests":[],"input_tokens":1,"cached_input_tokens":0,"output_tokens":2,"#,
    );
    let refused = serde_json::from_str::<super::ReportRequest<'_>>(&old)
        .err()
        .map(|error| error.to_string());
    assert!(
        refused
            .as_deref()
            .is_some_and(|reason| reason.contains("input_tokens")),
        "{refused:?}"
    );
    let renew = serde_json::from_str::<super::RenewRequest>(
        r#"{"input_tokens":1,"cached_input_tokens":0,"output_tokens":2}"#,
    );
    assert!(renew.is_err(), "a cumulative renewal is refused");
}
