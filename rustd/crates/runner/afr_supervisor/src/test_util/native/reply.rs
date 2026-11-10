//! Healthy native replies share the same wire fixtures as scripted unit peers.

use af_wire::protocol::Up;
use serde_json::{Value, json};

use crate::{
    Result,
    client::Verb,
    error::ErrorKind,
    test_util::{Healthy, RUNNER_ID},
};

pub(super) fn answer(healthy: &Healthy, message: &Up<'_>) -> Result<Option<Value>> {
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
            "lease_id":request.lease_id, "answer":ok(healthy, Verb::Report)}),
        Up::Folds {
            request_id,
            lease_id,
            ..
        } => json!({"type":"folded", "request_id":request_id,
            "lease_id":lease_id, "answer":ok(healthy, Verb::Folds)}),
        Up::RenewBatch { request_id, leases } => json!({"type":"renewed", "request_id":request_id,
            "leases":leases.iter().map(|lease| json!({"lease_id":lease.lease_id,
                "answer":ok(healthy, Verb::Renew)})).collect::<Vec<_>>()}),
        Up::Ready { .. } | Up::Activity { .. } => return Ok(None),
        _ => return Err(ErrorKind::StreamProtocol.into()),
    };
    Ok(Some(response))
}

fn ok(healthy: &Healthy, verb: Verb) -> Value {
    json!({"status":"ok", "response":healthy.to(Some(verb))})
}
