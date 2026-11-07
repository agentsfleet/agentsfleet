//! Human controls retain the entire accepted goal and cumulative measurements.

use afd_core::{error_code, id::Uuid7};
use afd_fleet::task::{TaskControl, TaskLifecycle as _, TaskSnapshot};
use afd_wire::task::TaskState;

use super::fixture::{Case, exercise};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn pause_resume_and_cancel_preserve_progress_and_enqueue_only_real_transitions() {
    exercise(control).await;
}

async fn control(case: &Case) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let bindings = tasks
        .bind(case.lease(), &case.plan("controlled-goals", 3))
        .await
        .expect("initial goals");
    let task = bindings
        .items
        .first()
        .expect("active task")
        .task_id
        .as_ref()
        .expect("accepted new-work item owns a task");
    sqlx::query("UPDATE fleet.tasks SET used_model_nanos=50000,reserved_model_nanos=100000,used_running_nanos=5000000000,used_decision_calls=2 WHERE id=$1::uuid")
        .bind(task.as_str()).execute(&mut *case.held.fixtures.database.acquire().await.expect("usage connection"))
        .await.expect("earlier measured work");
    let initial = tasks
        .read(scope, task)
        .await
        .expect("measured current goal");
    let paused = pause(case, task, initial).await;
    let resumed = resume(case, task, paused).await;
    cancel(case, task, resumed).await;
}

async fn pause(case: &Case, task: &Uuid7, initial: TaskSnapshot) -> TaskSnapshot {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let mut expected = initial.clone();
    expected.context.state = TaskState::Paused;
    expected.context.generation += 1;
    expected.context.stop_generation += 1;
    expected.resource_version += 1;
    let paused = tasks
        .control(
            scope,
            task,
            &initial.etag(),
            initial.context.generation,
            TaskControl::Pause,
        )
        .await
        .expect("pause");
    assert_eq!(
        paused, expected,
        "pause preserves all goals, cursors, budget and usage"
    );
    assert_eq!(
        tasks
            .control(
                scope,
                task,
                &paused.etag(),
                paused.context.generation,
                TaskControl::Pause
            )
            .await
            .expect("idempotent pause"),
        paused
    );
    assert_eq!(case.counts().await, (3, 3, 3));

    paused
}

async fn resume(case: &Case, task: &Uuid7, paused: TaskSnapshot) -> TaskSnapshot {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let mut expected = paused.clone();
    expected.context.state = TaskState::Ready;
    expected.context.generation += 1;
    expected.resource_version += 1;
    let resumed = tasks
        .control(
            scope,
            task,
            &paused.etag(),
            paused.context.generation,
            TaskControl::Resume,
        )
        .await
        .expect("resume");
    assert_eq!(resumed, expected);
    assert_eq!(
        case.counts().await,
        (3, 3, 4),
        "one generation-bound continuation"
    );
    assert_eq!(
        tasks
            .control(
                scope,
                task,
                &resumed.etag(),
                resumed.context.generation,
                TaskControl::Resume
            )
            .await
            .expect("idempotent ready"),
        resumed
    );
    let stale = tasks
        .control(
            scope,
            task,
            &paused.etag(),
            paused.context.generation,
            TaskControl::Resume,
        )
        .await
        .expect_err("old version cannot repeat a transition");
    assert_eq!(stale.code(), error_code::TASK_PRECONDITION_FAILED);
    assert_eq!(case.counts().await, (3, 3, 4));

    resumed
}

async fn cancel(case: &Case, task: &Uuid7, resumed: TaskSnapshot) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let mut expected = resumed.clone();
    expected.context.state = TaskState::Cancelled;
    expected.context.generation += 1;
    expected.context.stop_generation += 1;
    expected.resource_version += 1;
    let cancelled = tasks
        .control(
            scope,
            task,
            &resumed.etag(),
            resumed.context.generation,
            TaskControl::Cancel,
        )
        .await
        .expect("cancel");
    assert_eq!(cancelled, expected);
    assert_eq!(
        tasks
            .control(
                scope,
                task,
                &cancelled.etag(),
                cancelled.context.generation,
                TaskControl::Cancel
            )
            .await
            .expect("idempotent cancel"),
        cancelled
    );
    terminal_is_retained(case, task, &cancelled).await;
}

async fn terminal_is_retained(case: &Case, task: &Uuid7, cancelled: &TaskSnapshot) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    for action in [TaskControl::Pause, TaskControl::Resume] {
        let denied = tasks
            .control(
                scope,
                task,
                &cancelled.etag(),
                cancelled.context.generation,
                action,
            )
            .await
            .expect_err("terminal work cannot restart");
        assert_eq!(denied.code(), error_code::TASK_STALE_STATE);
    }
    assert_eq!(
        tasks
            .read(scope, task)
            .await
            .expect("terminal retained read"),
        *cancelled
    );
    assert_eq!(
        tasks
            .list(scope, None, 16)
            .await
            .expect("unfinished work list")
            .items
            .len(),
        2
    );
    assert_eq!(case.counts().await, (3, 3, 4));
}
