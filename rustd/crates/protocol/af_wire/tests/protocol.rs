//! Bounds and billing acknowledgements are identical on either carrier.

use af_wire::protocol::{Answer, Down, RUNS_MAX, StreamKind, Up};
use garde::Validate as _;
use serde::de::Error as _;
use serde_json::{error::Category, json};

const RUN: &str = "019a3c00-0000-7000-8000-000000000001";

fn required<T>(value: Option<T>) -> serde_json::Result<T> {
    value.ok_or_else(|| serde_json::Error::custom("expected protocol test value"))
}

#[test]
fn an_up_frame_with_an_unknown_field_is_refused() -> serde_json::Result<()> {
    let valid: Up<'_> = serde_json::from_str(r#"{"type":"ping","request_id":1}"#)?;
    assert_eq!(valid.request_id().map(std::num::NonZeroU64::get), Some(1));
    let invalid = r#"{"type":"ping","request_id":1,"extra":true}"#;
    let error = required(serde_json::from_str::<Up<'_>>(invalid).err())?;
    assert!(error.to_string().contains("extra"));
    Ok(())
}

#[test]
fn a_request_identifier_cannot_be_zero_or_negative() -> serde_json::Result<()> {
    for request_id in ["0", "-1", "null", "\"1\""] {
        let body = format!(r#"{{"type":"ping","request_id":{request_id}}}"#);
        let error = required(serde_json::from_str::<Up<'_>>(&body).err())?;
        assert_eq!(error.classify(), Category::Data);
    }
    Ok(())
}

#[test]
fn a_renew_batch_past_max_workers_is_refused() -> serde_json::Result<()> {
    for (count, allowed) in [(RUNS_MAX, true), (RUNS_MAX + 1, false), (0, false)] {
        let entry = json!({"run_id": RUN, "request": {"requests": []}});
        let body = json!({"type": "renew_batch", "request_id": 1, "runs": vec![entry; count]});
        let encoded = body.to_string();
        let decoded: Up<'_> = serde_json::from_str(&encoded)?;
        if allowed {
            decoded.validate().map_err(serde_json::Error::custom)?;
        } else {
            let report = required(decoded.validate().err())?;
            assert!(report.to_string().contains("runs"));
        }
    }
    Ok(())
}

#[test]
fn hello_bounds_running_runs_and_credits() -> serde_json::Result<()> {
    for (running, credits, allowed) in [
        (RUNS_MAX, RUNS_MAX, true),
        (RUNS_MAX + 1, 0, false),
        (0, RUNS_MAX + 1, false),
    ] {
        let body = json!({
            "type": "hello", "request_id": 1, "accepted_kinds": ["fleet", "authoring"],
            "running": vec![RUN; running], "credits": credits,
        });
        let encoded = body.to_string();
        let decoded: Up<'_> = serde_json::from_str(&encoded)?;
        if allowed {
            decoded.validate().map_err(serde_json::Error::custom)?;
        } else {
            let report = required(decoded.validate().err())?;
            let field = if running > RUNS_MAX {
                "running"
            } else {
                "credits"
            };
            assert!(report.to_string().contains(field));
        }
    }
    Ok(())
}

#[test]
fn renewal_answers_preserve_each_usage_acknowledgement() -> serde_json::Result<()> {
    let body = json!({
        "type": "renewed", "request_id": 12,
        "runs": [
            {"run_id": RUN, "answer": {"status": "ok", "response": {
                "run_expires_at": "2026-11-08T10:00:00.125Z", "recorded_through": 9
            }}},
            {"run_id": RUN, "answer": {
                "status": "refused", "error_code": "UZ-RUN-022", "detail": "This run was cancelled.", "retryable": false
            }}
        ]
    });
    let encoded = body.to_string();
    let decoded: Down<'_> = serde_json::from_str(&encoded)?;
    let Down::Renewed { request_id, runs } = &decoded else {
        return Err(serde_json::Error::custom("expected renewal answer"));
    };
    assert_eq!(request_id.get(), 12);
    let [first, second] = runs.as_slice() else {
        return Err(serde_json::Error::custom("two run answers expected"));
    };
    let Answer::Ok { response } = &first.answer else {
        return Err(serde_json::Error::custom("first run renews"));
    };
    assert_eq!(response.recorded_through, Some(9));
    let Answer::Refused { retryable, .. } = &second.answer else {
        return Err(serde_json::Error::custom("second run is refused"));
    };
    assert!(!retryable);
    assert_eq!(serde_json::to_value(&decoded)?, body);
    Ok(())
}

#[test]
fn message_kinds_select_their_stream_and_pending_answer() -> serde_json::Result<()> {
    let ping: Up<'_> = serde_json::from_str(r#"{"type":"ping","request_id":7}"#)?;
    assert_eq!(ping.stream_kind(), StreamKind::Control);
    assert_eq!(ping.request_id().map(std::num::NonZeroU64::get), Some(7));
    let ready: Up<'_> =
        serde_json::from_str(r#"{"type":"ready","credits":1,"revision":1,"observed_offers":0}"#)?;
    assert_eq!(ready.request_id(), None);
    let cancel: Down<'_> = serde_json::from_str(
        r#"{"type":"cancel","run_id":"019a3c00-0000-7000-8000-000000000001","error_code":"UZ-RUN-022"}"#,
    )?;
    assert_eq!(cancel.stream_kind(), StreamKind::Control);
    assert_eq!(cancel.request_id(), None);
    Ok(())
}

#[test]
fn unknown_stream_kinds_are_never_substituted() {
    for kind in StreamKind::ALL {
        assert_eq!(StreamKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(StreamKind::parse("unknown"), None);
}
