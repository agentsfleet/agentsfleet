//! Atomic multi-goal acceptance, concurrent replay and native ownership refusal.

use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicUsize, Ordering},
};

use af_core::error_code;
use af_wire::durable_task::{DurableTaskLimits, DurableTaskState, DurableTaskUsage};
use afd_fleet::durable_task::{
    DurableTaskBindings, DurableTaskLifecycle as _, DurableTaskRun, DurableTaskScope, PreparedPlan,
};
use futures_util::future::join_all;
use tokio::sync::Barrier;

use super::fixture::{Case, GOALS, exercise, questions};

const CALLERS: usize = 100;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_plan_replays_preserve_every_goal() {
    exercise(bind_and_replay).await;
}

async fn bind_and_replay(case: &Case) {
    let plan = case.plan("accepted-three-goals", GOALS.len());
    let replies = contended_calls(case, &plan).await;
    assert_eq!(replies.iter().filter(|reply| !reply.replayed).count(), 1);
    let first = replies.first().expect("100 replies");
    assert_eq!(first.items.len(), 3);
    assert!(replies.iter().all(|reply| reply.items == first.items));
    assert!(
        replies
            .iter()
            .all(|reply| reply.execution_deadline == first.execution_deadline)
    );
    stored_deadline(case, first).await;
    let warm_replies = contended_calls(case, &plan).await;
    assert!(warm_replies.iter().all(|reply| {
        reply.replayed
            && reply.items == first.items
            && reply.execution_deadline == first.execution_deadline
    }));
    let unique: BTreeSet<_> = first
        .items
        .iter()
        .map(|item| {
            item.durable_task_id
                .as_ref()
                .expect("accepted new-work item owns a task")
                .as_str()
        })
        .collect();
    assert_eq!(unique.len(), 3);
    assert_eq!(
        case.counts().await,
        (3, 3, 4),
        "three ready tasks, each with its own queued continuation"
    );
    assert_eq!(
        decision_calls(case).await,
        0,
        "no binding makes a decision call"
    );

    accepted_goals(case, first).await;
    replay_and_refuse_a_changed_plan(case, &plan, first).await;
}

async fn contended_calls(case: &Case, plan: &PreparedPlan) -> Vec<DurableTaskBindings> {
    let tasks = &case.tasks;
    let barrier = Barrier::new(CALLERS);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let replies = join_all((0..CALLERS).map(|_| {
        let barrier = &barrier;
        let active = &active;
        let peak = &peak;
        async move {
            barrier.wait().await;
            let in_flight = active.fetch_add(1, Ordering::Relaxed) + 1;
            peak.fetch_max(in_flight, Ordering::Relaxed);
            let result = tasks
                .bind(case.run(), plan)
                .await
                .expect("complete plan binds");
            active.fetch_sub(1, Ordering::Relaxed);
            result
        }
    }))
    .await;
    assert!(peak.load(Ordering::Relaxed) > 1, "native calls overlapped");
    assert_eq!(active.load(Ordering::Relaxed), 0);
    replies
}

async fn accepted_goals(case: &Case, bindings: &DurableTaskBindings) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    for (binding, goal) in bindings.items.iter().zip(GOALS) {
        let durable_task_id = binding
            .durable_task_id
            .as_ref()
            .expect("accepted goal owns a task");
        let snapshot = tasks
            .read(scope, durable_task_id)
            .await
            .expect("accepted goal");
        let current = &snapshot.context;
        assert!(
            !binding.applies_here,
            "the calling run stays an ordinary run"
        );
        assert_eq!(
            (
                snapshot.title.as_str(),
                current.objective.as_ref(),
                current.durable_task_id.as_ref()
            ),
            (goal, goal, durable_task_id.as_str())
        );
        assert_eq!(current.questions, questions());
        assert_eq!(
            (
                current.state,
                current.generation,
                current.stop_generation,
                current.usage,
                current.limits,
                snapshot.resource_version
            ),
            (
                DurableTaskState::Ready,
                1,
                0,
                DurableTaskUsage::default(),
                DurableTaskLimits::default(),
                1
            )
        );
    }
}

/// The decision calls the fleet's runs have made.
async fn decision_calls(case: &Case) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM runner.decision_calls WHERE fleet_id = $1")
        .bind(&case.held.fleet)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("decision call connection"),
        )
        .await
        .expect("decision calls are countable")
}

async fn replay_and_refuse_a_changed_plan(
    case: &Case,
    plan: &PreparedPlan,
    first: &DurableTaskBindings,
) {
    let tasks = &case.tasks;
    let replay = tasks
        .bind(case.run(), plan)
        .await
        .expect("the same call answers the first");
    assert!(replay.replayed);
    assert_eq!(replay.items, first.items);
    assert_eq!(replay.execution_deadline, first.execution_deadline);
    let changed = case.plan("changed-meaning", 3);
    let refusal = tasks
        .bind(case.run(), &changed)
        .await
        .expect_err("changed replay cannot allocate work");
    assert_eq!(refusal.code(), error_code::DURABLE_TASK_REPLAY_CONFLICT);
    assert_eq!(case.counts().await, (3, 3, 4));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn foreign_runners_fences_and_scopes_cannot_mutate_accepted_work() {
    exercise(ownership).await;
}

async fn wrong_attempts(case: &Case, plan: &PreparedPlan) {
    let tasks = &case.tasks;
    let wrong_runner = tasks
        .bind(
            DurableTaskRun {
                runner_id: &case.held.spare,
                ..case.run()
            },
            plan,
        )
        .await
        .expect_err("a peer runner owns no part of this source");
    assert_eq!(wrong_runner.code(), error_code::RUN_NOT_FOUND);
    assert_eq!(case.counts().await, (0, 0, 1));
    let stale = tasks
        .bind(
            DurableTaskRun {
                fencing_token: case.run().fencing_token + 1,
                ..case.run()
            },
            plan,
        )
        .await
        .expect_err("a different fence owns no work");
    assert_eq!(stale.code(), error_code::RUN_STALE_FENCING_TOKEN);
    assert_eq!(case.counts().await, (0, 0, 1));
}

async fn ownership(case: &Case) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let plan = case.plan("owned-goals", 2);
    wrong_attempts(case, &plan).await;
    let accepted = tasks
        .bind(case.run(), &plan)
        .await
        .expect("current owner still binds");
    assert_eq!(accepted.items.len(), 2);
    let foreign = DurableTaskScope {
        workspace_id: afd_db::test_util::mint_id(),
        ..case.scope.clone()
    };
    let task = accepted
        .items
        .first()
        .expect("first accepted task")
        .durable_task_id
        .as_ref()
        .expect("accepted new-work item owns a task");
    let refused = tasks
        .read(&foreign, task)
        .await
        .expect_err("foreign scope cannot discover the task");
    assert_eq!(refused.code(), error_code::DURABLE_TASK_NOT_FOUND);
    assert_eq!(case.counts().await, (2, 2, 3));
    assert_eq!(
        tasks
            .read(scope, task)
            .await
            .expect("owned read recovers")
            .context
            .state,
        DurableTaskState::Ready
    );
}

/// A binding leaves the calling run unbound, so it carries no task deadline,
/// on its reply or its row.
async fn stored_deadline(case: &Case, reply: &DurableTaskBindings) {
    let stored: Option<jiff_sqlx::Timestamp> = sqlx::query_scalar(
        "SELECT durable_task_execution_deadline_at FROM thread.runs WHERE id=$1",
    )
    .bind(case.run().run_id)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("deadline connection"),
    )
    .await
    .expect("the run row");
    assert_eq!(stored, None);
    assert_eq!(reply.execution_deadline, None);
}
