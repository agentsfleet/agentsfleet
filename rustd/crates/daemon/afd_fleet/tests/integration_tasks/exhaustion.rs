//! Exhausted work stays readable and cannot gain another lease by resuming.

use af_core::{error_code, id::Uuid7};
use af_wire::task::{TaskLimits, TaskState};
use afd_fleet::task::{TaskControl, TaskLifecycle as _, TaskSnapshot};

use super::fixture::{Case, exercise};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn cumulative_limits_and_wall_expiry_refuse_resume_without_erasing_progress() {
    exercise(exhausted).await;
}

async fn exhausted(case: &Case) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let accepted = tasks
        .bind(case.lease(), &case.plan("bounded-goals", 3))
        .await
        .expect("initial goals");
    let source = case.claimed_source().await;
    let fourth = tasks
        .bind(
            case.lease(),
            &case.source_plan(source, "fourth-goal", 1, TaskLimits::default()),
        )
        .await
        .expect("fourth independent goal");
    let all = accepted.items.into_iter().chain(fourth.items);
    for (binding, resource) in all.zip(["model", "running", "leases", "wall"]) {
        refused_resume(
            case,
            binding.task_id.as_ref().expect("accepted goal owns a task"),
            resource,
        )
        .await;
    }
    let page = tasks
        .list(scope, None, 16)
        .await
        .expect("unfinished work read");
    assert_eq!(page.items, Vec::new(), "cancelled work leaves the list");
    assert!(!page.more);
    assert_eq!(case.counts().await, (4, 4, 5));
}

async fn refused_resume(case: &Case, task: &Uuid7, resource: &str) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let initial = tasks.read(scope, task).await.expect("accepted goal");
    tasks
        .control(
            scope,
            task,
            &initial.etag(),
            initial.context.generation,
            TaskControl::Pause,
        )
        .await
        .expect("pause before exhaustion");
    sqlx::query("UPDATE fleet.tasks SET used_model_nanos=CASE WHEN $2='model' THEN max_model_nanos-100 ELSE 50 END, reserved_model_nanos=CASE WHEN $2='model' THEN 100 ELSE 0 END, used_running_nanos=CASE WHEN $2='running' THEN max_running_nanos ELSE 10 END, used_leases=CASE WHEN $2='leases' THEN max_leases ELSE used_leases END, created_at=CASE WHEN $2='wall' THEN created_at-interval '8 days' ELSE created_at END, expires_at=CASE WHEN $2='wall' THEN expires_at-interval '8 days' ELSE expires_at END WHERE id=$1")
        .bind(task).bind(resource)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("usage connection"))
        .await.expect("retained cumulative usage");
    let before = tasks
        .read(scope, task)
        .await
        .expect("saved exhausted snapshot");
    assert_eq!(before.context.state, TaskState::Paused);
    assert_eq!(before.context.objective, initial.context.objective);
    assert_eq!(before.context.questions, initial.context.questions);
    let counts = case.counts().await;
    for _attempt in 0..2 {
        let failure = tasks
            .control(
                scope,
                task,
                &before.etag(),
                before.context.generation,
                TaskControl::Resume,
            )
            .await
            .expect_err("resume cannot replenish this resource");
        assert_eq!(
            failure.code(),
            error_code::TASK_LIMIT_EXHAUSTED,
            "{resource}"
        );
        assert_eq!(
            tasks
                .read(scope, task)
                .await
                .expect("progress survives refusal"),
            before
        );
        assert_eq!(
            case.counts().await,
            counts,
            "no continuation is admitted for {resource}"
        );
    }
    cancel_exhausted(case, task, before, counts).await;
}

async fn cancel_exhausted(
    case: &Case,
    task: &Uuid7,
    before: TaskSnapshot,
    counts: (i64, i64, i64),
) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let cancelled = tasks
        .control(
            scope,
            task,
            &before.etag(),
            before.context.generation,
            TaskControl::Cancel,
        )
        .await
        .expect("exhausted work can still be stopped");
    let mut expected = before;
    expected.context.state = TaskState::Cancelled;
    expected.context.generation += 1;
    expected.context.stop_generation += 1;
    expected.resource_version += 1;
    assert_eq!(
        cancelled, expected,
        "stopping exhausted work retains all measured usage"
    );
    assert_eq!(case.counts().await, counts);
}
