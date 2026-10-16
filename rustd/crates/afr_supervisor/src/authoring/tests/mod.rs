//! Run the real authoring worker and encrypted drainer with a daemon seam.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test fixtures fail loudly"
)]

mod execution;
mod failures;

use crate::{
    client::{Call, Verb},
    test_support::{Answer, Behaviour, FakeAgent, FakeEngine, Rig, json},
};
use afd_wire::authoring::Lease;
use serde_json::{Value, json};

const LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8060";
const ATTEMPT: &str = "01890a5d-ac96-774b-bcce-b302099a8061";

fn lease() -> Lease<Value> {
    serde_json::from_value(json!({
        "lease_id": LEASE, "attempt_id": ATTEMPT, "draft_id": LEASE,
        "sequence": "1", "fence": "1", "deletion_generation": "0",
        "expires_at": 30_000, "deadline": 300_000,
        "model": {"ref":"owned", "provider":"openai", "model":"gpt-4.1",
            "tokenizer":"o200k_base", "pricing":{"input_nanos_per_million":"1",
                "output_nanos_per_million":"1", "runtime_nanos_per_second":"1"}},
        "model_access":{"token":"a".repeat(43),"expires_at":30_000,
            "invocation_path":"https://untrusted.invalid/never-follow-this"},
        "messages":[{"role":"user","text":"Investigate checkout"}],
        "limits":{"input_per_call":32768,"output_per_turn":8192,"max_calls":8,
            "wall_seconds":300,"provider_timeout_seconds":30}
    }))
    .unwrap()
}

fn rig(answer: impl Fn(&Call) -> Answer + Send + Sync + 'static) -> Rig {
    Rig::new(
        answer,
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    )
}

fn result(call: &Call, proposals: &Value, cut: bool) -> Answer {
    let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
    json(
        &json!({"dispatch_id": request["dispatch_id"], "call_index":request["call_index"],
        "replayed":false,"text":"Private result", "proposal_calls":proposals, "cut":cut,
        "usage":{"knowledge":"known","input_tokens":3,"output_tokens":2,"provider_cost_nanos":"1"}}),
    )
}

fn ack() -> Answer {
    json(&json!({"settled":true,"replayed":false,"sequence":"1","state":"completed"}))
}

async fn run(rig: &mut Rig, lease: Lease<Value>) -> (Vec<Call>, Value) {
    rig.lessee.author(&lease).await.unwrap();
    let lessee = std::sync::Arc::clone(&rig.lessee);
    let draining = tokio::spawn(async move {
        lessee
            .authoring_spool
            .run(&lessee.plane, &lessee.halt)
            .await;
    });
    let mut calls = Vec::new();
    let report = loop {
        let call = tokio::time::timeout(std::time::Duration::from_secs(2), rig.calls.recv())
            .await
            .unwrap()
            .unwrap();
        if call.verb == Verb::AuthoringReport {
            let report = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            calls.push(call);
            break report;
        }
        calls.push(call);
    };
    rig.shutdown.cancel();
    draining.await.unwrap();
    (calls, report)
}
