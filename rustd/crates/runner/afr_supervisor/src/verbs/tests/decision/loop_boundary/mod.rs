//! Real root execution receives decision authority only through the fenced HTTP client.
mod fixture;
mod model;
mod server;

use super::{CALL, FENCING, FencedVerbs, Host, LEASE_ID};
use afr_agent::AgentEngine as _;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn native_pending_refused_and_stale_decisions_stop_root_without_completion() {
    for mode in [
        "reserved", "issued", "unknown", "not_sent", "refused", "stale",
    ] {
        exercise(mode).await;
    }
}

#[tokio::test]
async fn real_decision_client_preflight_is_receipted_before_root_can_use_it() {
    for mode in ["empty_credential", "invalid_header", "mismatched_receipt"] {
        exercise(mode).await;
    }
}

async fn exercise(mode: &'static str) {
    let server = server::Server::start(mode).await;
    let host = Host::new();
    let lease_id = af_core::id::Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(
        &server.plane,
        &lease_id,
        FENCING,
        &host.spool,
        &host.held,
        &host.stop,
    );
    let model = model::Model::default();
    let engine = afr_agent::Loop::new(afr_tools::Catalog::new(vec![]), model.clone());
    let lease = fixture::lease();
    let inputs = fixture::inputs();
    let prepared = afr_agent::PreparedTask {
        execution_deadline: inputs.execution_deadline,
        inputs,
        allow_actions: true,
    };
    let output = execute(&engine, &lease, &prepared, &verbs, &host).await;
    assert_stopped(&output, &model);
    server.assert_calls();
    server.stop();
}

fn assert_stopped(output: &afr_agent::RunOutput, model: &model::Model) {
    assert_eq!(
        model.0.load(Ordering::SeqCst),
        1,
        "root outcome: {:?}",
        output.result.outcome
    );
    assert!(output.task.is_none());
    assert!(output.recovery.is_some());
    assert_eq!(output.result.content, "");
    assert_eq!(
        output.result.outcome,
        afr_agent::ResultOutcome::Failed(afr_agent::Failure {
            class: None,
            detail: "the run was stopped before it finished".into(),
        })
    );
}

async fn execute(
    engine: &afr_agent::Loop,
    lease: &af_wire::lease::LeasePayload<'_>,
    prepared: &afr_agent::PreparedTask,
    verbs: &FencedVerbs<'_>,
    host: &Host,
) -> afr_agent::RunOutput {
    engine
        .run(afr_agent::AgentRun {
            lease,
            prepared_task: Some(prepared),
            recovered_task: None,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs,
            checkpoint: &afr_agent::testing::Discard,
            context_checkpoints: &afr_agent::testing::ContextMemory::default(),
            inbox: &afr_agent::testing::NoInbox,
            events: &|_| {},
            meter: &afr_agent::Meter::default(),
            stop: &host.stop,
        })
        .await
        .unwrap()
}
