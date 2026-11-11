//! Check completion between a paid WAIT decision and report preparation must not strand work.
use super::{
    super::{
        fixture::{Case, exercise, id},
        report_settlement,
    },
    checkpoint,
    fixture::Decisions,
    github_authority,
    github_server::Server,
    publication_refusal,
};
use af_wire::task::{CheckpointMode, DecisionTaskVersion, TaskReport, TaskState};
use afd_fleet::task::{TaskLifecycle as _, Tasks};
use serde_json::Value;
use std::sync::Arc;

mod maintenance;
mod support;
use support::{Observation, checkpoint_row, counts, decision, observe};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn checks_finishing_after_paid_wait_requeue_once_for_success_and_failure() {
    for observation in [Observation::Success, Observation::Failure] {
        exercise(async |case| {
            Box::pin(scenario(case, observation, false, TaskState::Ready)).await;
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn pending_or_missing_checks_preserve_paid_wait_and_checkpoint() {
    for observation in [Observation::Pending, Observation::Missing] {
        exercise(async |case| {
            Box::pin(scenario(case, observation, false, TaskState::Waiting)).await;
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn finished_checks_cannot_bypass_head_open_or_checkpoint_guards() {
    for (observation, invalid_checkpoint) in [
        (Observation::ChangedHead, false),
        (Observation::Closed, false),
        (Observation::Success, true),
    ] {
        exercise(async |case| {
            Box::pin(scenario(
                case,
                observation,
                invalid_checkpoint,
                TaskState::Blocked,
            ))
            .await;
        })
        .await;
    }
}

async fn scenario(
    case: &Case,
    observation: Observation,
    invalid_checkpoint: bool,
    expected: TaskState,
) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let server = Server::start(vec![]).await;
    let tasks = github_authority::tasks(&fixture, &server).await;
    // Reuse the accepted native PR/workflow fixture; publication itself is not performed.
    let _target = publication_refusal::prepared(case, task.clone()).await;
    let (checkpoint, saved) = capture(&fixture, &task).await;
    let mut semantic = decision(&fixture, task.clone(), &checkpoint).await;
    assert_eq!(
        fixture.counts().await,
        (1, 1, 1),
        "WAIT decision is already paid"
    );
    if invalid_checkpoint {
        semantic.checkpoint_id = Some(afd_db::test_util::mint_id().into());
    }
    // These responses become available only after the WAIT receipt committed.
    observe(&server, observation);
    settle(&fixture, &tasks, semantic, expected, &checkpoint, &saved).await;
    server.finish().await;
    fixture.model.finish().await.expect("model cleanup");
}

async fn capture(fixture: &Decisions<'_>, task: &DecisionTaskVersion) -> (String, Value) {
    let case = fixture.case;
    let storage = checkpoint::storage(fixture);
    let plan = storage
        .plan_checkpoint(
            case.lease(),
            &checkpoint::valid(checkpoint::input(
                task,
                case,
                CheckpointMode::RetainBase,
                None,
                &"0".repeat(40),
            )),
        )
        .await
        .expect("real checkpoint allocation");
    let base = checkpoint::complete(&storage, case, task, &plan).await;
    let plan = storage
        .plan_checkpoint(
            case.lease(),
            &checkpoint::valid(checkpoint::input(
                task,
                case,
                CheckpointMode::Incremental,
                base.base_id,
                &"0".repeat(40),
            )),
        )
        .await
        .expect("incremental recovery allocation");
    let plan = checkpoint::complete(&storage, case, task, &plan).await;
    let saved = checkpoint_row(case, &id(&plan.checkpoint_id)).await;
    (plan.checkpoint_id, saved)
}

async fn settle(
    fixture: &Decisions<'_>,
    tasks: &Tasks,
    semantic: TaskReport,
    expected: TaskState,
    checkpoint: &str,
    saved: &Value,
) {
    let case = fixture.case;
    let task = semantic.task.clone();
    let task_id = id(&task.task_id);
    let report = report_settlement::request(case, Some(semantic));
    let mut plane = case.held.fixtures.plane();
    plane.task_reports = Some(Arc::new(tasks.clone()));
    let now = af_core::clock::after(case.held.now, crate::report_seed::SLICE);
    plane
        .report(
            &case.held.runner,
            &report,
            now,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("ordinary report settlement");
    let snapshot = tasks
        .read(&case.scope, &task_id)
        .await
        .expect("settled task");
    support::assert_state(&snapshot, expected, checkpoint);
    let after = counts(case, &task_id).await;
    support::assert_counts(after, expected);
    assert_eq!(checkpoint_row(case, &id(checkpoint)).await, *saved);
    plane
        .report(
            &case.held.runner,
            &report,
            now,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("duplicate report replay");
    assert_eq!(
        tasks
            .read(&case.scope, &task_id)
            .await
            .expect("stable task"),
        snapshot
    );
    assert_eq!(counts(case, &task_id).await, after);
    assert_eq!(
        fixture.counts().await,
        (1, 1, 1),
        "replay creates no decision call or charge"
    );
}
