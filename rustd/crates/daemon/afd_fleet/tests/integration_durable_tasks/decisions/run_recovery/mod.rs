//! Reassigned durable work uses the ordinary plane and its current setup proof.
use super::{
    super::fixture::{exercise, id},
    fixture::Decisions,
};
use af_wire::durable_task::DurableTaskState;
use afd_fleet::durable_task::DurableTaskLifecycle as _;
use serde_json::Value;
pub(super) mod support;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reassigned_durable_task_attaches_to_one_ordinary_run_with_original_generation_and_deadline()
 {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let default = support::provider(&fixture).await;
        support::model(&fixture, Some(&fixture.model.model.id)).await;
        let task = fixture.bind_goal().await;
        let durable_task_id = id(&task.durable_task_id);
        support::expire(case).await;
        let acquired = support::assign(case).await;
        let fence = acquired.fence.as_i64();
        let run = reassigned(case, acquired).await;
        assert_eq!(
            run.pointer("/durable_task/durable_task_id"),
            Some(&serde_json::json!(task.durable_task_id))
        );
        assert_eq!(
            run.pointer("/durable_task/generation"),
            Some(&serde_json::json!(1))
        );
        assert_eq!(
            run.pointer("/durable_task/state"),
            Some(&serde_json::json!("running"))
        );
        let run_id = id(run
            .get("run_id")
            .and_then(Value::as_str)
            .expect("issued ordinary run"));
        assert_ne!(run_id, case.held.run_key);
        support::assert_issued(case, &durable_task_id, &run_id, fence).await;
        assert_eq!(
            fixture
                .tasks
                .read(&case.scope, &durable_task_id)
                .await
                .expect("issued task")
                .context
                .state,
            DurableTaskState::Running
        );
        support::model(&fixture, None).await;
        drop(default);
        fixture.model.finish().await.expect("model fixture cleanup");
    })
    .await;
}

/// The cap that ends a plain event after its runs keep lapsing leaves a
/// durable task alone: the task's own run allowance bounds it, and ending
/// its event would strand the task as running with nothing left to run it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_durable_task_whose_runs_lapsed_past_the_plain_cap_is_assigned_again() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let default = support::provider(&fixture).await;
        support::model(&fixture, Some(&fixture.model.model.id)).await;
        let task = fixture.bind_goal().await;
        let durable_task_id = id(&task.durable_task_id);
        support::expire(case).await;
        for _holder in 1..af_core::limits::DEFAULT_MAX_RUNS_PER_TURN {
            let run = reassigned(case, support::assign(case).await).await;
            let lapsed = id(run
                .get("run_id")
                .and_then(Value::as_str)
                .expect("issued ordinary run"));
            case.held
                .fixtures
                .lapse_holder(&lapsed, &case.held.fleet)
                .await;
        }

        let acquired = support::assign(case).await;
        let counted = acquired.earlier_runs;
        let run = reassigned(case, acquired).await;

        assert_eq!(
            counted,
            i64::from(af_core::limits::DEFAULT_MAX_RUNS_PER_TURN),
            "the event is at the plain cap"
        );
        assert_eq!(
            run.pointer("/durable_task/durable_task_id"),
            Some(&serde_json::json!(task.durable_task_id))
        );
        assert_eq!(
            fixture
                .tasks
                .read(&case.scope, &durable_task_id)
                .await
                .expect("issued task")
                .context
                .state,
            DurableTaskState::Running
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
pub(super) async fn reassigned(
    case: &super::super::fixture::Case,
    acquired: afd_fleet::run::Acquired,
) -> Value {
    let capture = af_core::test_util::trace::Capture::install();
    let body = case
        .held
        .fixtures
        .plane()
        .assign_selected(acquired, &case.held.runner, af_core::clock::now())
        .await
        .expect("ordinary reassignment and full issuance");

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
