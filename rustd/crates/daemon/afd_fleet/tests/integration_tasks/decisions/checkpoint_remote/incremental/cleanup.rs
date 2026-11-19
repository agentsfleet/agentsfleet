//! Collect one abandoned overlay while its base and committed overlay stay live.
use super::{Capture, Case, DecisionTaskVersion, Tasks, id};
use afd_fleet::task::{CheckpointMaintenanceCursor, TaskLifecycle as _};
pub(super) async fn collect(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    orphan: &Capture,
) {
    age(case, task, orphan).await;
    let mut cursor = CheckpointMaintenanceCursor::default();
    let removed = tasks
        .maintain_checkpoints(&mut cursor)
        .await
        .expect("incremental orphan cleanup");
    assert_eq!(
        (
            removed.owners,
            removed.bases,
            removed.objects,
            removed.purged,
            removed.deferred
        ),
        (1, 0, 1, 1, 0)
    );
    let state: (i64, i64, bool, bool) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM fleet.task_bases WHERE id=$3),
          (SELECT count(*) FROM fleet.task_checkpoints WHERE task_id=$1),
          EXISTS(SELECT 1 FROM fleet.task_objects WHERE id=$2),
          EXISTS(SELECT 1 FROM fleet.task_storage_purges WHERE task_object_id=$2 AND completed_at IS NOT NULL)",
    ).bind(id(&task.task_id)).bind(id(&orphan.object.object_id))
     .bind(orphan.plan.base_id.as_deref().map(id))
     .fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("custody reader"))
     .await.expect("orphan private state erased");
    assert_eq!(state, (1, 2, false, true));
    let again = tasks
        .maintain_checkpoints(&mut cursor)
        .await
        .expect("idempotent collection");
    assert_eq!((again.objects, again.purged), (0, 0));
}

async fn age(case: &Case, task: &DecisionTaskVersion, orphan: &Capture) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("expiry fixture");
    sqlx::query("UPDATE fleet.task_object_pins SET expires_at=now()-interval '2 hours',operation_expires_at=now()-interval '2 hours' WHERE task_checkpoint_id=$1")
        .bind(id(&orphan.plan.checkpoint_id)).execute(&mut *connection).await.expect("orphan pin expiry");
    sqlx::query("UPDATE fleet.task_checkpoints SET transfer_expires_at=now()-interval '2 hours' WHERE id=$1")
        .bind(id(&orphan.plan.checkpoint_id)).execute(&mut *connection).await.expect("orphan plan expiry");
    sqlx::query("UPDATE fleet.task_objects SET updated_at=now()-interval '2 hours',reclaim_after_at=now()-interval '2 hours' WHERE id=$1")
        .bind(id(&orphan.object.object_id)).execute(&mut *connection).await.expect("orphan object age");
    sqlx::query(
        "UPDATE fleet.tasks SET checkpoint_cleanup_at=now()-interval '2 hours' WHERE id=$1",
    )
    .bind(id(&task.task_id))
    .execute(&mut *connection)
    .await
    .expect("due task cleanup");
}
