//! Assertions use actual exported trace/parent IDs and production connection attributes.

use super::{Failure, Result, observe::Evidence};
use af_core::id::Uuid7;
use serde_json::Value;

pub(super) fn check(evidence: &Evidence, url: &str) -> Result<(), Failure> {
    let spans = spans(&evidence.exports);
    let connections: Vec<_> = spans
        .iter()
        .copied()
        .filter(|span| text(span, "name") == Some("runner.connection"))
        .collect();
    assert!(
        connections.len() >= 3,
        "established, failed reopening and recovered attempts all export"
    );
    let first = connections
        .iter()
        .copied()
        .find(|span| attribute(span, "recovery") == Some("initial"))
        .ok_or("initial connection missing")?;
    let restored = connections
        .iter()
        .copied()
        .find(|span| attribute(span, "recovery") == Some("restored"))
        .ok_or("recovered connection missing")?;
    assert_connections(&connections, first, restored)?;
    assert_attempts(&spans, first, restored);
    assert_logs(
        evidence,
        attribute(first, "attempt_id").ok_or("attempt ID missing")?,
        attribute(restored, "attempt_id").ok_or("restored ID missing")?,
    )?;
    assert_redaction(evidence, &spans, url)?;
    Ok(())
}

fn assert_connections(
    connections: &[&Value],
    first: &Value,
    restored: &Value,
) -> Result<(), Failure> {
    Uuid7::parse(attribute(first, "attempt_id").ok_or("first attempt ID missing")?)?;
    Uuid7::parse(attribute(restored, "connection_id").ok_or("confirmed recovered owner missing")?)?;
    assert_ne!(
        attribute(first, "attempt_id"),
        attribute(restored, "attempt_id")
    );
    assert_eq!(attribute(first, "transport"), Some("http2"));
    assert_eq!(attribute(restored, "transport"), Some("http2"));
    assert_eq!(attribute(first, "outcome"), Some("retrying"));
    assert_eq!(attribute(restored, "outcome"), Some("stopped"));
    let outage = attributes(restored)
        .into_iter()
        .find(|field| text(field, "key") == Some("disconnected_seconds"))
        .and_then(|field| field.pointer("/value/doubleValue"))
        .and_then(Value::as_f64)
        .ok_or("recovery duration missing")?;
    assert!(outage > 0.0 && outage < 30.0);
    assert!(
        connections
            .iter()
            .any(|span| attribute(span, "connection_id").is_none()
                && attribute(span, "outcome") == Some("retrying"))
    );
    Ok(())
}

fn assert_redaction(evidence: &Evidence, spans: &[&Value], url: &str) -> Result<(), Failure> {
    let exported = serde_json::to_string(&evidence.exports)?;
    for private in [
        "agt_r_private_observability_fixture",
        "private-frame-fixture",
        url,
    ] {
        assert!(!evidence.logs.contains(private));
        assert!(!exported.contains(private));
    }
    assert!(
        !spans
            .iter()
            .any(|span| text(span, "name") == Some("unreviewed_payload"))
    );
    Ok(())
}

fn assert_attempts(spans: &[&Value], first: &Value, restored: &Value) {
    let under = |root: &Value, operation: &str, carrier: Option<&str>, success: bool| {
        spans.iter().any(|span| {
            text(span, "traceId") == text(root, "traceId")
                && text(span, "parentSpanId") == text(root, "spanId")
                && attribute(span, "operation") == Some(operation)
                && carrier.is_none_or(|carrier| attribute(span, "transport") == Some(carrier))
                && attribute(span, "outcome").is_some_and(|outcome| {
                    if success {
                        outcome == "success"
                    } else {
                        matches!(outcome, "network" | "timeout" | "closed" | "unsupported")
                    }
                })
        })
    };
    assert!(under(first, "connection_attempt", Some("http3"), false));
    assert!(under(first, "connection_attempt", Some("http2"), true));
    assert!(
        under(restored, "write", None, true),
        "recovered protocol writes keep the real parent"
    );
    assert_finite_outcomes(spans);
}

fn assert_finite_outcomes(spans: &[&Value]) {
    for span in spans
        .iter()
        .filter(|span| text(span, "name") == Some("runner_stream_operation"))
    {
        if let Some(outcome) = attribute(span, "outcome") {
            assert!(matches!(
                outcome,
                "success"
                    | "network"
                    | "timeout"
                    | "cancelled"
                    | "protocol"
                    | "closed"
                    | "worker"
                    | "tls"
                    | "unsupported"
                    | "abandoned"
            ));
        }
    }
}

fn assert_logs(evidence: &Evidence, first: &str, restored: &str) -> Result<(), Failure> {
    let entries: Vec<Value> = evidence
        .logs
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    let correlated = |event: &str, attempt: &str| {
        entries.iter().find(|entry| {
            entry.pointer("/fields/event").and_then(Value::as_str) == Some(event)
                && entry
                    .get("spans")
                    .and_then(Value::as_array)
                    .is_some_and(|spans| {
                        spans
                            .iter()
                            .any(|span| text(span, "attempt_id") == Some(attempt))
                    })
        })
    };
    let fallback =
        correlated("runner_stream_fallback_attempt", first).ok_or("correlated fallback missing")?;
    assert_eq!(
        fallback
            .pointer("/fields/transport")
            .and_then(Value::as_str),
        Some("http2")
    );
    assert!(matches!(
        fallback.pointer("/fields/reason").and_then(Value::as_str),
        Some("network" | "timeout" | "closed" | "unsupported")
    ));
    assert!(correlated("runner_stream_operation_failed", first).is_some());
    assert!(correlated("runner_stream_reconnecting", first).is_some());
    let recovered =
        correlated("runner_stream_recovered", restored).ok_or("correlated recovery missing")?;
    assert_eq!(
        recovered
            .pointer("/fields/recovery")
            .and_then(Value::as_str),
        Some("restored")
    );
    Ok(())
}

fn spans(exports: &[Value]) -> Vec<&Value> {
    exports
        .iter()
        .flat_map(|export| array(export, "resourceSpans"))
        .flat_map(|resource| array(resource, "scopeSpans"))
        .flat_map(|scope| array(scope, "spans"))
        .collect()
}
fn array<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|array| array.iter().collect())
        .unwrap_or_default()
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
fn attributes(value: &Value) -> Vec<&Value> {
    array(value, "attributes")
}
fn attribute<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    attributes(value)
        .into_iter()
        .find(|attribute| text(attribute, "key") == Some(key))
        .and_then(|attribute| attribute.pointer("/value/stringValue"))
        .and_then(Value::as_str)
}
