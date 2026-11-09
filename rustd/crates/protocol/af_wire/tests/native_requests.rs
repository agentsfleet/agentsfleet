//! Native commands retain strict bodies and usage bounds after their HTTP adapters are removed.

use af_wire::{
    fold::FoldRequest,
    protocol::Up,
    usage::{LINES_MAX, RUNNER_SEQ_MAX},
};
use garde::Validate as _;
use serde_json::{Value, error::Category, json};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
const LEASE: &str = "01924f4e-0000-7000-8000-00000000a11e";

#[test]
fn folds_refuse_missing_malformed_and_extra_fields() -> TestResult {
    for body in [
        "",
        "{",
        "null",
        "[]",
        "{}",
        r#"{"fencing_token":7}"#,
        r#"{"after_sequence":0}"#,
        r#"{"fencing_token":-1,"after_sequence":0}"#,
        r#"{"fencing_token":7,"after_sequence":"0"}"#,
        r#"{"fencing_token":7,"after_sequence":0,"extra":1}"#,
    ] {
        let error = serde_json::from_str::<FoldRequest>(body)
            .err()
            .ok_or("an unreadable fold was accepted")?;
        assert!(
            matches!(
                error.classify(),
                Category::Eof | Category::Syntax | Category::Data
            ),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn native_folds_validate_the_nested_cursor_and_lease_bound() -> TestResult {
    for (lease, cursor, allowed, field) in [
        (LEASE, 0, true, ""),
        (LEASE, i64::MAX, true, ""),
        (LEASE, -1, false, "after_sequence"),
        ("x", 0, false, "lease_id"),
    ] {
        let bytes = json!({"type":"folds", "request_id":1, "lease_id":lease,
            "request":{"fencing_token":7,"after_sequence":cursor}})
        .to_string();
        let command: Up<'_> = serde_json::from_str(&bytes)?;
        if allowed {
            command.validate()?;
        } else {
            let error = command
                .validate()
                .err()
                .ok_or("an invalid native fold was accepted")?;
            assert!(error.to_string().contains(field), "{error}");
        }
    }
    Ok(())
}

#[test]
fn native_renewals_refuse_malformed_and_retired_request_shapes() -> TestResult {
    for request in [
        "",
        "not-json",
        "null",
        "[]",
        r#"{"requests":"x"}"#,
        r#"{"requests":[],"input_tokens":1,"cached_input_tokens":2,"output_tokens":3}"#,
    ] {
        let body = format!(
            r#"{{"type":"renew_batch","request_id":1,"leases":[{{"lease_id":"{LEASE}","request":{request}}}]}}"#
        );
        let error = serde_json::from_str::<Up<'_>>(&body)
            .err()
            .ok_or("an invalid renewal was accepted")?;
        assert!(
            matches!(error.classify(), Category::Syntax | Category::Data),
            "{request}"
        );
    }
    Ok(())
}

fn usage(seq: u32) -> Value {
    json!({"seq":seq,"at":"2025-10-09T08:00:00Z","input_tokens":10,
        "cached_input_tokens":0,"cache_write_tokens":0,"cache_write_5m_tokens":0,
        "cache_write_1h_tokens":0,"output_tokens":5})
}

#[test]
fn native_renewals_keep_request_line_count_and_sequence_bounds() -> TestResult {
    for (count, sequence, allowed) in [
        (0, 0, true),
        (LINES_MAX, RUNNER_SEQ_MAX, true),
        (LINES_MAX + 1, 0, false),
        (1, RUNNER_SEQ_MAX + 1, false),
    ] {
        let bytes = json!({"type":"renew_batch","request_id":1,"leases":[{
            "lease_id":LEASE,"request":{"requests":vec![usage(sequence);count]}}]})
        .to_string();
        let command: Up<'_> = serde_json::from_str(&bytes)?;
        if allowed {
            command.validate()?;
        } else {
            let error = command
                .validate()
                .err()
                .ok_or("invalid native usage was accepted")?;
            assert!(error.to_string().contains("requests"), "{error}");
        }
    }
    Ok(())
}

#[test]
fn native_report_tool_traces_borrow_the_frame_and_preserve_its_answer() -> TestResult {
    for trace in [
        json!({"calls":[],"omitted_call_count":0}),
        json!(7),
        json!({"extra":1}),
    ] {
        let bytes = report(&trace).to_string();
        let command: Up<'_> = serde_json::from_str(&bytes)?;
        let Up::Report {
            request_id,
            request,
        } = command
        else {
            return Err("the report discriminator changed".into());
        };
        assert_eq!(request_id.get(), 9);
        assert!(matches!(
            request.response_text,
            std::borrow::Cow::Borrowed("private answer")
        ));
        assert_eq!(
            request.tool_calls.ok_or("the trace was lost")?.byte_len(),
            trace.to_string().len()
        );
        assert_eq!(
            serde_json::to_value(request.as_ref())?.get("tool_calls"),
            Some(&trace)
        );
    }
    Ok(())
}

fn report(trace: &Value) -> Value {
    json!({"type":"report","request_id":9,"request":{
        "lease_id":LEASE,"event_id":LEASE,"fencing_token":7,"outcome":"processed",
        "failure_reason":null,"failure_detail":"","response_text":"private answer",
        "tokens":0,"requests":[],"telemetry":{"time_to_first_token_ms":0,"wall_ms":1},
        "checkpoint":{"last_event_id":LEASE,"last_response":""},"tool_calls":trace
    }})
}

#[test]
fn a_malformed_native_tool_trace_preserves_the_report_and_is_rejected_on_its_own() -> TestResult {
    for trace in [json!(7), json!(["invalid"]), json!({"extra":1})] {
        let bytes = report(&trace).to_string();
        let Up::Report { request, .. } = serde_json::from_str::<Up<'_>>(&bytes)? else {
            return Err("a report became another command".into());
        };
        let failure = request
            .tool_calls
            .ok_or("the trace was lost")?
            .narrow()
            .err()
            .ok_or("an invalid trace was accepted")?;
        assert_eq!(failure, af_wire::tool_trace::TraceRejection::Malformed);
        assert_eq!(request.response_text, "private answer");
    }
    Ok(())
}

#[test]
fn native_reports_refuse_extra_frame_and_payload_fields() -> TestResult {
    for nested in [false, true] {
        let mut value = report(&json!({"calls":[],"omitted_call_count":0}));
        let target = if nested {
            value
                .get_mut("request")
                .ok_or("the report request is missing")?
        } else {
            &mut value
        };
        target
            .as_object_mut()
            .ok_or("the report must be an object")?
            .insert("extra".to_owned(), json!(true));
        let bytes = value.to_string();
        let error = serde_json::from_str::<Up<'_>>(&bytes)
            .err()
            .ok_or("an extra report field was accepted")?;
        assert_eq!(error.classify(), Category::Data);
        assert!(error.to_string().contains("extra"));
    }
    Ok(())
}
