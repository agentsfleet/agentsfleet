//! Real Loop → Lessee → HTTP checkpoint orchestration, with only external services scripted.
//! Stored-object reuse here does not prove remote persistence or replacement restoration.
use crate::test_support::{FENCING, FLEET_ID, FakeEngine, LEASE_ID, Rig, lease};
use serde_json::Value;
use std::sync::{Arc, atomic::Ordering};
mod archive;
mod fixture;
mod model;
mod server;
use archive::Refusal;
const ID: &str = "019f0000-0000-7000-8000-000000000001";
const CHECKPOINT: &str = "019f0000-0000-7000-8000-000000000002";
const PAGE: &str = "019f0000-0000-7000-8000-000000000003";

fn payload() -> afd_wire::lease::LeasePayload<'static> {
    let mut lease = lease(LEASE_ID, FLEET_ID);
    lease.event.workspace_id = ID.into();
    lease.durable_tasks = true;
    lease.decision = Some(Box::new(fixture::assignment()));
    lease.policy.tools.clear();
    lease.policy.context.context_cap_tokens = 64000;
    lease.policy.context.tool_window = 64;
    lease.policy.model_context_tokens = 64000;
    lease.policy.context.memory_checkpoint_every = 100;
    lease
}
async fn run(refusal: Refusal) -> (server::Server, Rig, model::Model) {
    let mut server = server::Server::start(refusal).await;
    let model = model::Model::default();
    let agent = afr_agent::Loop::new(afr_tools::Catalog::new(vec![]), model.clone());
    let mut rig = Rig::running(
        |_| unreachable!("all control-plane requests must use real HTTP"),
        FakeEngine::default(),
        agent,
    );
    Arc::get_mut(&mut rig.lessee).unwrap().plane = server.plane.take().unwrap();
    rig.run(&payload()).await.unwrap();
    assert_eq!(
        model.0.load(Ordering::SeqCst),
        1,
        "native fixture requests: {:?}",
        server.archive.lock().unwrap().calls
    );
    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        0,
        "API-only work needs no sandbox"
    );
    assert_eq!(
        crate::report_spool::ReportSpool::new(&rig.home)
            .pending()
            .await
            .unwrap()
            .len(),
        0
    );
    (server, rig, model)
}
fn report(archive: &archive::Archive) -> &Value {
    let reports: Vec<_> = archive
        .calls
        .iter()
        .filter(|(path, _)| path == afd_wire::paths::RUNNER_REPORTS)
        .collect();
    assert_eq!(reports.len(), 1);
    &reports.first().unwrap().1
}
#[tokio::test]
async fn real_loop_commits_api_history_before_reporting_reused_checkpoint() {
    let (server, _rig, _model) = run(Refusal::None).await;
    {
        let archive = server.archive.lock().unwrap();
        let report = report(&archive);
        assert_eq!(
            report.pointer("/task/disposition").and_then(Value::as_str),
            Some("waiting")
        );
        assert_eq!(
            report
                .pointer("/task/checkpoint_id")
                .and_then(Value::as_str),
            Some(CHECKPOINT)
        );
        assert_eq!(
            report
                .pointer("/task/decision_call_id")
                .and_then(Value::as_str),
            archive.decision.as_deref()
        );
        assert_eq!(
            report
                .pointer("/task/task/generation")
                .and_then(Value::as_u64),
            Some(7)
        );
        let committed = archive
            .calls
            .iter()
            .position(|(path, _)| path.ends_with(CHECKPOINT))
            .unwrap();
        let reported = archive
            .calls
            .iter()
            .position(|(path, _)| path == afd_wire::paths::RUNNER_REPORTS)
            .unwrap();
        assert!(committed < reported);
        assert!(archive.plan.as_ref().unwrap().history_digest.is_some());
        assert_eq!(archive.descriptors.len(), 1);
        assert_safe(&archive);
    };
    server.stop();
}
#[tokio::test]
async fn real_loop_checkpoint_refusal_clears_goal_authority_before_report() {
    for refusal in [Refusal::Plan, Refusal::Commit] {
        let (server, _rig, _model) = run(refusal).await;
        {
            let archive = server.archive.lock().unwrap();
            let report = report(&archive);
            assert_eq!(
                report.pointer("/task/disposition").and_then(Value::as_str),
                Some("blocked")
            );
            assert!(
                report
                    .pointer("/task/checkpoint_id")
                    .is_none_or(Value::is_null)
            );
            assert!(
                report
                    .pointer("/task/decision_call_id")
                    .is_none_or(Value::is_null)
            );
            assert!(
                report
                    .get("response_text")
                    .and_then(Value::as_str)
                    .unwrap()
                    .contains("No automatic continuation was authorized")
            );
            assert!(
                archive.decision.is_some(),
                "a real goal receipt existed before capture failed"
            );
            if refusal == Refusal::Plan {
                assert!(archive.page.is_none());
            } else {
                assert!(archive.page.is_some());
                assert!(archive.plan.as_ref().unwrap().manifest_root.is_none());
            }
            assert_safe(&archive);
        };
        server.stop();
    }
}
fn assert_safe(archive: &archive::Archive) {
    for (_, body) in &archive.calls {
        assert!(body.get("history").is_none());
        assert!(body.get("data_key").is_none());
        assert!(body.get("nonce").is_none());
        assert!(!body.to_string().contains("agt_r_recovery_fixture"));
    }
}
