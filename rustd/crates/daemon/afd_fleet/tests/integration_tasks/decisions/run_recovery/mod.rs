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
async fn reclaimed_task_attaches_to_one_ordinary_run_with_original_generation_and_deadline() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let default = support::provider(&fixture).await;
        support::model(&fixture, Some(&fixture.model.model.id)).await;
        let task = fixture.bind_goal().await;
        let task_id = id(&task.task_id);
        support::expire(case).await;
        let acquired = support::claim(case).await;
        let fence = acquired.fence.as_i64();
        let run = reclaimed(case, acquired).await;
        assert_eq!(
            run.pointer("/task/task_id"),
            Some(&serde_json::json!(task.task_id))
        );
        assert_eq!(run.pointer("/task/generation"), Some(&serde_json::json!(1)));
        assert_eq!(
            run.pointer("/task/state"),
            Some(&serde_json::json!("running"))
        );
        let run_id = id(run
            .get("run_id")
            .and_then(Value::as_str)
            .expect("issued ordinary run"));
        assert_ne!(run_id, case.held.run_key);
        support::assert_issued(case, &task_id, &run_id, fence).await;
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
async fn changed_setup_blocks_reclaimed_task_without_issuing_another_run() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let task_id = id(&task.task_id);
        support::expire(case).await;
        // The setup changes before the reclaim, which reads it in its claim.
        let config = serde_json::json!({"name":"pull-request-repair","x-agentsfleet":{"tools":[],"triggers":[],
            "budget":{"daily_nanos":"900000000"},"decision_policy":super::fixture::policy()}});
        case.held.fixtures.native_config(&case.held.fleet, &config.to_string()).await;
        let acquired = support::claim(case).await;
        let body = case.held.fixtures.plane().claim_selected(acquired, &case.held.runner, af_core::clock::now()).await.expect("changed authority is a native refusal");
        assert!(body.is_none());
        support::assert_blocked(case, &task_id).await;
        fixture.model.finish().await.expect("model fixture cleanup");
    }).await;
}

/// The cap that ends a plain event after its runs keep lapsing leaves a
/// durable task alone: the task's own run allowance bounds it, and ending
/// its event would strand the task as running with nothing left to run it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_whose_runs_lapsed_past_the_plain_cap_is_assigned_again() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let default = support::provider(&fixture).await;
        support::model(&fixture, Some(&fixture.model.model.id)).await;
        let task = fixture.bind_goal().await;
        let task_id = id(&task.task_id);
        support::expire(case).await;
        for _holder in 1..af_core::limits::EVENT_RUNS_MAX {
            let run = reclaimed(case, support::claim(case).await).await;
            let lapsed = id(run
                .get("run_id")
                .and_then(Value::as_str)
                .expect("issued ordinary run"));
            case.held
                .fixtures
                .lapse_holder(&lapsed, &case.held.fleet)
                .await;
        }

        let acquired = support::claim(case).await;
        let counted = acquired.earlier_runs;
        let run = reclaimed(case, acquired).await;

        assert_eq!(
            counted,
            i64::from(af_core::limits::EVENT_RUNS_MAX),
            "the event is at the plain cap"
        );
        assert_eq!(
            run.pointer("/task/task_id"),
            Some(&serde_json::json!(task.task_id))
        );
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

#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
pub(super) async fn reclaimed(
    case: &super::super::fixture::Case,
    acquired: afd_fleet::run::Acquired,
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
                Some("runner_run_no_work" | "runner_run_refused")
            )
        })
        .filter_map(|event| event.field("reason").map(str::to_owned))
        .collect();
    drop(capture);
    assert!(body.is_some(), "ordinary task run refused: {reasons:?}");
    body.expect("the ordinary task run was issued")
}
