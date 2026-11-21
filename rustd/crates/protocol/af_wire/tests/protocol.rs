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
            "type": "hello", "request_id": 1, "accepted_kinds": ["fleet", "setup"],
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
    let ready: Up<'_> = serde_json::from_str(
        r#"{"type":"ready","credits":1,"revision":1,"observed_assignments":0}"#,
    )?;
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

#[test]
fn the_runner_protocol_is_v4_alone() {
    assert_eq!(af_wire::protocol::VERSION, "agentsfleet.runner.v4");
}

/// The tags serde lists when it refuses `refused`'s unknown frame: every tag
/// the decoded enum has, read from the derive rather than written here.
fn listed_tags(refused: Option<serde_json::Error>) -> serde_json::Result<Vec<String>> {
    let text = required(refused)?.to_string();
    let listed = text
        .split_once("expected one of ")
        .map(|(_, rest)| rest)
        .ok_or_else(|| serde_json::Error::custom(format!("no tag list in {text}")))?;
    let listed = listed.split(" at line ").next().unwrap_or(listed);
    Ok(listed
        .split(", ")
        .map(|tag| tag.trim_matches('`').to_owned())
        .collect())
}

/// The daemon makes no model call, so no frame in either
/// direction carries one. The four setup model frames each way are gone, and
/// a runner still sending one is refused as an unknown frame.
#[test]
fn the_runner_protocol_has_no_setup_model_frames() -> serde_json::Result<()> {
    let unknown = r#"{"type":"no_such_frame"}"#;
    let down = listed_tags(serde_json::from_str::<Down<'_>>(unknown).err())?;
    let up = listed_tags(serde_json::from_str::<Up<'_>>(unknown).err())?;
    assert!(down.iter().any(|tag| tag == "run"), "{down:?}");
    assert!(up.iter().any(|tag| tag == "context_save"), "{up:?}");
    for tag in down.iter().chain(&up) {
        assert!(
            !tag.contains("authoring") && !tag.contains("model"),
            "{tag} carries a model call"
        );
    }
    for frame in [
        "authoring_model",
        "authoring_activity",
        "authoring_renew",
        "authoring_report",
    ] {
        let body = json!({"type": frame, "request_id": 1, "run_id": RUN, "request": {}});
        let refused = serde_json::from_str::<Up<'_>>(&body.to_string()).err();
        assert!(
            required(refused)?.to_string().contains("unknown variant"),
            "{frame}"
        );
    }
    for frame in [
        "authoring_model_result",
        "authoring_activity_ack",
        "authoring_renewed",
        "authoring_report_ack",
    ] {
        let body = json!({"type": frame, "request_id": 1, "run_id": RUN,
            "answer": {"status": "ok", "response": {}}});
        let refused = serde_json::from_str::<Down<'_>>(&body.to_string()).err();
        assert!(
            required(refused)?.to_string().contains("unknown variant"),
            "{frame}"
        );
    }
    Ok(())
}

#[test]
fn the_context_verb_rides_the_work_stream_and_answers_its_request() -> serde_json::Result<()> {
    let load_text = json!({"type": "context_load", "request_id": 3,
        "run_id": RUN, "request": {"fencing_token": 2}})
    .to_string();
    let load: Up<'_> = serde_json::from_str(&load_text)?;
    let save_text = json!({"type": "context_save", "request_id": 4,
        "run_id": RUN, "request": {"fencing_token": 2, "resource_version": 1,
            "checkpoint": {"generation": 2}}})
    .to_string();
    let save: Up<'_> = serde_json::from_str(&save_text)?;
    for (up, id) in [(&load, 3), (&save, 4)] {
        up.validate().map_err(serde_json::Error::custom)?;
        assert_eq!(up.stream_kind(), StreamKind::Work);
        assert_eq!(up.request_id().map(std::num::NonZeroU64::get), Some(id));
    }
    let loaded_text = json!({"type": "context_loaded",
        "request_id": 3, "run_id": RUN, "answer": {"status": "ok", "response": {
            "scope": "draft:x", "resource_version": 0}}})
    .to_string();
    let loaded: Down<'_> = serde_json::from_str(&loaded_text)?;
    let Down::ContextLoaded {
        answer: Answer::Ok { response },
        ..
    } = &loaded
    else {
        return Err(serde_json::Error::custom("the load did not answer"));
    };
    assert_eq!(
        (response.resource_version, response.checkpoint.is_none()),
        (0, true)
    );
    assert_eq!(loaded.stream_kind(), StreamKind::Work);
    let refused_text = json!({"type": "context_saved",
        "request_id": 4, "run_id": RUN, "answer": {"status": "refused",
            "error_code": "UZ-RUN-005", "detail": "superseded", "retryable": false}})
    .to_string();
    let refused: Down<'_> = serde_json::from_str(&refused_text)?;
    assert!(matches!(
        refused,
        Down::ContextSaved {
            answer: Answer::Refused { .. },
            ..
        }
    ));
    assert_eq!(refused.request_id().map(std::num::NonZeroU64::get), Some(4));
    Ok(())
}

#[test]
fn a_context_fence_no_thread_can_hold_is_refused() -> serde_json::Result<()> {
    let past = u64::MAX;
    let load_text = json!({"type": "context_load", "request_id": 1,
        "run_id": RUN, "request": {"fencing_token": past}})
    .to_string();
    let load: Up<'_> = serde_json::from_str(&load_text)?;
    let report = required(load.validate().err())?;
    assert!(report.to_string().contains("fencing_token"), "{report}");
    Ok(())
}

#[test]
fn a_context_save_never_prints_its_checkpoint() {
    let save = af_wire::context::ContextSaveRequest {
        fencing_token: 2,
        resource_version: 1,
        checkpoint: json!({"summary": {"goal": "a private goal"}}),
    };
    let printed = format!("{save:?}");
    assert!(!printed.contains("a private goal"), "{printed}");
    assert!(printed.contains("resource_version"), "{printed}");
}
