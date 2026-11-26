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
use af_wire::durable_task::{
    CheckpointMode, DecisionDurableTaskVersion, DurableTaskReport, DurableTaskState,
};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};
use serde_json::Value;
use std::sync::Arc;

mod support;
use support::{Observation, checkpoint_row, completion, continuing, counts, decision, observe};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn checks_finishing_after_paid_wait_requeue_once_for_success_and_failure() {
    for observation in [Observation::Success, Observation::Failure] {
        exercise(async |case| {
            Box::pin(scenario(case, observation, false, DurableTaskState::Ready)).await;
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn pending_or_missing_checks_preserve_paid_wait_and_checkpoint() {
    for observation in [Observation::Pending, Observation::Missing] {
        exercise(async |case| {
            Box::pin(scenario(
                case,
                observation,
                false,
                DurableTaskState::Waiting,
            ))
            .await;
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
                DurableTaskState::Blocked,
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
    expected: DurableTaskState,
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

async fn capture(fixture: &Decisions<'_>, task: &DecisionDurableTaskVersion) -> (String, Value) {
    let case = fixture.case;
    let storage = checkpoint::storage(fixture);
    let plan = storage
        .plan_checkpoint(
            case.run(),
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
            case.run(),
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
    tasks: &DurableTasks,
    semantic: DurableTaskReport,
    expected: DurableTaskState,
    checkpoint: &str,
    saved: &Value,
) {
    let case = fixture.case;
    let task = semantic.durable_task.clone();
    let durable_task_id = id(&task.durable_task_id);
    let report = report_settlement::request(case, Some(semantic));
    let mut plane = case.held.fixtures.plane();
    plane.durable_task_reports = Some(Arc::new(tasks.clone()));
    let now = af_core::clock::after(case.held.now, crate::report_seed::SLICE);
    plane
        .report_with(
            &case.held.runner,
            &report,
            now,
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("ordinary report settlement");
    let snapshot = tasks
        .read(&case.scope, &durable_task_id)
        .await
        .expect("settled task");
    support::assert_state(&snapshot, expected, checkpoint);
    let after = counts(case, &durable_task_id).await;
    support::assert_counts(after, expected);
    assert_eq!(checkpoint_row(case, &id(checkpoint)).await, *saved);
    plane
        .report_with(
            &case.held.runner,
            &report,
            now,
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("duplicate report replay");
    assert_eq!(
        tasks
            .read(&case.scope, &durable_task_id)
            .await
            .expect("stable task"),
        snapshot
    );
    assert_eq!(counts(case, &durable_task_id).await, after);
    assert_eq!(
        fixture.counts().await,
        (1, 1, 1),
        "replay creates no decision call or charge"
    );
}

// Value: protects=held statements against a bound pull request finish on green
// checks at its head, wait re-armed while they run, and block on red ones;
// fails_when=a completion settles Done without its checks, blocks while they
// run, or completes on a red one; seam=the production report over live
// Postgres, the GitHub fixture answering the head and its checks.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_pull_request_completion_waits_for_running_checks_and_finishes_only_on_green() {
    for (observation, expected, armed) in [
        (Observation::Success, DurableTaskState::Done, 0),
        (Observation::Pending, DurableTaskState::Waiting, 1),
        (Observation::Missing, DurableTaskState::Waiting, 1),
        (Observation::Failure, DurableTaskState::Blocked, 0),
    ] {
        exercise(async |case| {
            let fixture = Decisions::create(case).await;
            let task = fixture.bind_goal().await;
            let server = Server::start(vec![]).await;
            let tasks = github_authority::tasks(&fixture, &server).await;
            let _target = publication_refusal::prepared(case, task.clone()).await;
            let semantic = completion(&fixture, task.clone()).await;
            let durable_task_id = id(&semantic.durable_task.durable_task_id);
            observe(&server, observation);
            assert_eq!(completed(case, &tasks, semantic).await, expected);
            assert_eq!(
                counts(case, &durable_task_id).await.2,
                armed,
                "a running check re-arms the wait: {expected:?}"
            );
            server.finish().await;
            fixture.model.finish().await.expect("model cleanup");
        })
        .await;
    }
}

// Value: protects=a completion whose head cannot be read, or whose deployment
// holds no GitHub reader, is never prepared and blocks as unverified;
// fails_when=an unread dependency lets the completion settle Done;
// why_new=the unread-dependency arms of report preparation had no case;
// seam=the production report over live Postgres, the GitHub fixture failing
// the head read.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_pull_request_completion_without_readable_checks_blocks() {
    for reader in [true, false] {
        exercise(async |case| {
            let fixture = Decisions::create(case).await;
            let task = fixture.bind_goal().await;
            let server = Server::start(vec![]).await;
            let configured = github_authority::tasks(&fixture, &server).await;
            let _target = publication_refusal::prepared(case, task.clone()).await;
            let semantic = completion(&fixture, task.clone()).await;
            let tasks = if reader {
                server.push(serde_json::json!({
                    "fixture_request":{"method":"GET","path":"/repos/example/linkwarden/pulls/101"},
                    "response":{"fixture_http_status":500,"fixture_http_body":{"message":"unavailable"}}}));
                configured
            } else {
                fixture.tasks.clone()
            };
            assert_eq!(
                completed(case, &tasks, semantic).await,
                DurableTaskState::Blocked,
                "reader configured: {reader}"
            );
            if reader {
                server.finish().await;
            } else {
                // No reader, so the App exchange was never reached to finish.
                drop(server);
            }
            fixture.model.finish().await.expect("model cleanup");
        })
        .await;
    }
}

/// Reports `semantic` through the ordinary report, returning the state settled.
async fn completed(
    case: &Case,
    tasks: &DurableTasks,
    semantic: DurableTaskReport,
) -> DurableTaskState {
    let durable_task_id = id(&semantic.durable_task.durable_task_id);
    let report = report_settlement::request(case, Some(semantic));
    let mut plane = case.held.fixtures.plane();
    plane.durable_task_reports = Some(Arc::new(tasks.clone()));
    plane
        .report_with(
            &case.held.runner,
            &report,
            af_core::clock::after(case.held.now, crate::report_seed::SLICE),
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await
        .expect("ordinary report settlement");
    tasks
        .read(&case.scope, &durable_task_id)
        .await
        .expect("settled task")
        .context
        .state
}

// Value: protects=a paid CONTINUE from the task's saved checkpoint queues
// exactly one continuation at the next generation; fails_when=a continuation
// with current evidence blocks, or queues twice on replay; why_new=the
// continue arm of settlement had no case; seam=the production report over live
// Postgres, a real committed checkpoint.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_paid_continue_from_its_checkpoint_queues_one_continuation() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let server = Server::start(vec![]).await;
        let tasks = github_authority::tasks(&fixture, &server).await;
        let _target = publication_refusal::prepared(case, task.clone()).await;
        let (checkpoint, saved) = capture(&fixture, &task).await;
        let semantic = continuing(&fixture, task, &checkpoint).await;
        Box::pin(settle(
            &fixture,
            &tasks,
            semantic,
            DurableTaskState::Ready,
            &checkpoint,
            &saved,
        ))
        .await;
        // A continuation reads no check, so the App exchange is never reached.
        drop(server);
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
