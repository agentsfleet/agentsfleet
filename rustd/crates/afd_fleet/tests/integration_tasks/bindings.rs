//! Atomic multi-goal acceptance, concurrent replay and native ownership refusal.

use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicUsize, Ordering},
};

use afd_core::error_code;
use afd_fleet::task::{PreparedPlan, TaskBindings, TaskLease, TaskLifecycle as _, TaskScope};
use afd_wire::task::{TaskLimits, TaskState, TaskUsage};
use futures_util::future::join_all;
use tokio::sync::Barrier;

use super::fixture::{Case, GOALS, exercise, id, questions};

const CALLERS: usize = 100;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_plan_replays_preserve_all_goals_and_their_original_revision() {
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
    let unique: BTreeSet<_> = first
        .items
        .iter()
        .map(|item| {
            item.task_id
                .as_ref()
                .expect("accepted new-work item owns a task")
                .as_str()
        })
        .collect();
    assert_eq!(unique.len(), 3);
    assert_eq!(
        case.counts().await,
        (3, 3, 3),
        "one active task and two queued continuations"
    );

    accepted_goals(case, first).await;
    replay_after_revision(case, &plan, first).await;
}

async fn contended_calls(case: &Case, plan: &PreparedPlan) -> Vec<TaskBindings> {
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
                .bind(case.lease(), plan)
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

async fn accepted_goals(case: &Case, bindings: &TaskBindings) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    for (index, (binding, goal)) in bindings.items.iter().zip(GOALS).enumerate() {
        let task_id = binding.task_id.as_ref().expect("accepted goal owns a task");
        let snapshot = tasks.read(scope, task_id).await.expect("accepted goal");
        let current = &snapshot.context;
        let state = if index == 0 {
            TaskState::Running
        } else {
            TaskState::Ready
        };
        let usage = TaskUsage {
            leases: u32::from(index == 0),
            ..TaskUsage::default()
        };
        assert_eq!(
            (
                snapshot.title.as_str(),
                current.objective.as_ref(),
                current.accepted_revision_id.as_ref(),
                current.task_id.as_ref()
            ),
            (goal, goal, case.revision.as_str(), task_id.as_str())
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
            (state, 1, 0, usage, TaskLimits::default(), 1)
        );
        assert_eq!(snapshot.accepted_revision_number, case.revision_number);
    }
}

async fn replay_after_revision(case: &Case, plan: &PreparedPlan, first: &TaskBindings) {
    let tasks = &case.tasks;
    case.held
        .fixtures
        .native_config(
            &case.held.fleet,
            r#"{"name":"updated-repair-setup","x-agentsfleet":{"tools":[],"triggers":[],"budget":{"daily_nanos":"1000000000"}}}"#,
        )
        .await;
    let replay = tasks
        .bind(case.lease(), plan)
        .await
        .expect("accepted identity survives a newer setup");
    assert!(replay.replayed);
    assert_eq!(replay.items, first.items);
    assert_eq!(replay.execution_deadline, first.execution_deadline);
    let changed = case.plan("changed-meaning", 3);
    let refusal = tasks
        .bind(case.lease(), &changed)
        .await
        .expect_err("changed replay cannot allocate work");
    assert_eq!(refusal.code(), error_code::TASK_REPLAY_CONFLICT);
    assert_eq!(case.counts().await, (3, 3, 3));
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
            TaskLease {
                runner_id: &case.held.spare,
                ..case.lease()
            },
            plan,
        )
        .await
        .expect_err("a peer runner owns no part of this source");
    assert_eq!(wrong_runner.code(), error_code::RUN_LEASE_NOT_FOUND);
    assert_eq!(case.counts().await, (0, 0, 1));
    let stale = tasks
        .bind(
            TaskLease {
                fencing_token: case.lease().fencing_token + 1,
                ..case.lease()
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
        .bind(case.lease(), &plan)
        .await
        .expect("current owner still binds");
    assert_eq!(accepted.items.len(), 2);
    let foreign = TaskScope {
        workspace_id: id(&afd_db::test_util::mint_id()),
        ..case.scope.clone()
    };
    let task = accepted
        .items
        .first()
        .expect("first accepted task")
        .task_id
        .as_ref()
        .expect("accepted new-work item owns a task");
    let refused = tasks
        .read(&foreign, task)
        .await
        .expect_err("foreign scope cannot discover the task");
    assert_eq!(refused.code(), error_code::TASK_NOT_FOUND);
    assert_eq!(case.counts().await, (2, 2, 2));
    assert_eq!(
        tasks
            .read(scope, task)
            .await
            .expect("owned read recovers")
            .context
            .state,
        TaskState::Running
    );
}

/// Native receipts preserve the database ceiling rather than restarting a slice on replay.
async fn stored_deadline(case: &Case, reply: &TaskBindings) {
    let stored: jiff_sqlx::Timestamp =
        sqlx::query_scalar("SELECT task_execution_deadline FROM runner.leases WHERE id=$1::uuid")
            .bind(case.lease().lease_id.as_str())
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
            .expect("bound lease deadline");
    assert_eq!(reply.execution_deadline, stored.to_jiff());
    let input = case
        .tasks
        .inputs(
            case.lease(),
            &garde::Unvalidated::new(afd_wire::task::TaskInputsQuery {
                fencing_token: case.lease().fencing_token,
                starting_after: None,
                after_input_sequence: None,
                limit: 4,
            })
            .validate()
            .expect("input query"),
        )
        .await
        .expect("recovery input read");
    assert_eq!(input.execution_deadline, Some(reply.execution_deadline));
}
