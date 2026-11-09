//! Reclaimed durable work uses the ordinary plane and its current setup proof.
use super::{
    super::fixture::{exercise, id},
    fixture::Decisions,
};
use af_wire::task::TaskState;
use afd_fleet::task::TaskLifecycle as _;
use serde_json::Value;
pub(super) mod support;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reclaimed_task_attaches_to_one_ordinary_lease_with_original_generation_and_deadline() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let default = support::provider(&fixture).await;
        support::model(&fixture, Some(&fixture.model.model.id)).await;
        let task = fixture.bind_goal().await;
        let task_id = id(&task.task_id);
        support::expire(case).await;
        let acquired = support::claim(case).await;
        let fence = acquired.fence.as_i64();
        let lease = reclaimed(case, acquired).await;
        assert_eq!(
            lease.pointer("/task/task_id"),
            Some(&serde_json::json!(task.task_id))
        );
        assert_eq!(
            lease.pointer("/task/generation"),
            Some(&serde_json::json!(1))
        );
        assert_eq!(
            lease.pointer("/task/state"),
            Some(&serde_json::json!("running"))
        );
        let lease_id = id(lease
            .get("lease_id")
            .and_then(Value::as_str)
            .expect("issued ordinary lease"));
        assert_ne!(lease_id, case.held.lease_key);
        support::assert_issued(case, &task_id, &lease_id, fence).await;
        assert_eq!(
            fixture
                .tasks
                .read(&case.scope, &task_id)
                .await
                .expect("issued task")
                .context
                .state,
            TaskState::Running
        );
        support::model(&fixture, None).await;
        drop(default);
        fixture.model.finish().await.expect("model fixture cleanup");
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn changed_setup_blocks_reclaimed_task_without_issuing_another_lease() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let task_id = id(&task.task_id);
        support::expire(case).await;
        let acquired = support::claim(case).await;
        let config = serde_json::json!({"name":"pull-request-repair","x-agentsfleet":{"tools":[],"triggers":[],
            "budget":{"daily_nanos":"900000000"},"decision_policy":super::fixture::policy()}});
        case.held.fixtures.native_config(&case.held.fleet, &config.to_string()).await;
        let body = case.held.fixtures.plane().claim_selected(acquired, &case.held.runner, af_core::clock::now()).await.expect("changed authority is a native refusal");
        assert!(body.is_none());
        support::assert_blocked(case, &task_id).await;
        fixture.model.finish().await.expect("model fixture cleanup");
    }).await;
}

pub(super) async fn reclaimed(
    case: &super::super::fixture::Case,
    acquired: afd_fleet::lease::Acquired,
) -> Value {
    let capture = af_core::test_util::trace::Capture::install();
    let body = case
        .held
        .fixtures
        .plane()
        .claim_selected(acquired, &case.held.runner, af_core::clock::now())
        .await
        .expect("ordinary reclaim and full issuance");

    let reasons: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| {
            matches!(
                event.field("event"),
                Some("runner_lease_no_work" | "runner_lease_refused")
            )
        })
        .filter_map(|event| event.field("reason").map(str::to_owned))
        .collect();
    drop(capture);
    body.unwrap_or_else(|| panic!("ordinary task lease refused: {reasons:?}"))
}
