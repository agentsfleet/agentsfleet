//! Healthy native replies share the same wire fixtures as scripted unit peers.

use af_wire::protocol::Up;
use serde_json::{Value, json};

use crate::{
    client::Verb,
    test_util::{Healthy, RUNNER_ID},
};

pub(super) fn answer(healthy: &Healthy, message: &Up<'_>) -> Option<Value> {
    let response = match message {
        Up::Hello { request_id, .. } => json!({"type":"prepared", "request_id":request_id,
            "connection_id":RUNNER_ID, "previous_connection_id":null}),
        Up::Confirm {
            request_id,
            connection_id,
        } => json!({"type":"welcome", "request_id":request_id,
            "connection_id":connection_id, "heartbeat_interval_ms":healthy.interval_ms}),
        Up::Heartbeat { request_id, .. } => json!({"type":"heartbeat", "request_id":request_id,
            "answer":ok(healthy, Verb::Heartbeat)}),
        Up::Ping { request_id } => json!({"type":"pong", "request_id":request_id}),
        Up::Report {
            request_id,
            request,
        } => json!({"type":"report_ack", "request_id":request_id,
        "run_id":request.run_id, "answer":{"status":"ok", "response":af_wire::report::ReportResponse {
            ok: true, hold: request.held_until.map(|expires_at| af_wire::report::AcceptedHold {
                run_id: request.run_id.to_string(), fencing_token: request.fencing_token, expires_at,
            }),
        }}}),
        Up::Folds {
            request_id, run_id, ..
        } => json!({"type":"folded", "request_id":request_id,
            "run_id":run_id, "answer":ok(healthy, Verb::Folds)}),
        Up::RenewBatch { request_id, runs } => json!({"type":"renewed", "request_id":request_id,
            "runs":runs.iter().map(|run| json!({"run_id":run.run_id,
                "answer":ok(healthy, Verb::Renew)})).collect::<Vec<_>>()}),
        Up::Ready { .. } | Up::Progress { .. } => return None,
        Up::TurnCheckpoint {
            request_id,
            run_id,
            request,
        } => json!({"type":"turn_checkpoint", "request_id":request_id,
            "run_id":run_id, "operation_id":request.operation_id, "kind":request.action.kind(),
            "answer":{"status":"refused","error_code":af_core::error_code::RUN_CHECKPOINT_UNAVAILABLE.as_str(),
            "detail":"This scripted peer has no durable checkpoint store.","retryable":false}}),
        // A fresh fleet chat reads its thread's saved summary at its start.
        Up::ContextLoad {
            request_id, run_id, ..
        } => json!({"type":"context_loaded", "request_id":request_id,
            "run_id":run_id, "answer":ok(healthy, Verb::ContextLoad)}),
        // A save moves the stored row one generation past the one it names.
        Up::ContextSave {
            request_id,
            run_id,
            request,
        } => json!({"type":"context_saved", "request_id":request_id, "run_id":run_id,
            "answer":{"status":"ok", "response":af_wire::context::ContextSaveResponse {
                resource_version: request.resource_version.saturating_add(1),
            }}}),
    };
    Some(response)
}

fn ok(healthy: &Healthy, verb: Verb) -> Value {
    json!({"status":"ok", "response":healthy.to(Some(verb))})
}
