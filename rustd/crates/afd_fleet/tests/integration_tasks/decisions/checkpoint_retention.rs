//! Expired capture cleanup transfers custody before releasing storage quota.

use afd_fleet::task::{CheckpointMaintenanceCursor, TaskLifecycle as _};
use afd_wire::task::{CheckpointMode, CheckpointObjectsInput, CheckpointPlaintext};

use super::{
    super::fixture::{Case, exercise, id},
    checkpoint::{input, valid},
    checkpoint_remote::configured,
    checkpoint_server::Server,
    fixture::Decisions,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn expired_capture_erases_wrapped_keys_and_releases_quota_after_remote_delete_once() {
    exercise(retention).await;
}

async fn retention(case: &Case) {
    let server = Server::start().await;
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = configured(&fixture, &server).await;
    let (plan, objects) = capture(&tasks, case, &task).await;
    let object = &objects.items[0].object_id;
    let mut cursor = CheckpointMaintenanceCursor::default();
    let retained = tasks
        .maintain_checkpoints(&mut cursor)
        .await
        .expect("live custody survives");
    assert_eq!(retained.objects, 0);
    assert_eq!(retained.owners, 0);
    expire(case, &task.task_id, &plan, object).await;
    let removed = tasks
        .maintain_checkpoints(&mut cursor)
        .await
        .expect("expired capture cleanup");
    assert_eq!(
        (
            removed.owners,
            removed.bases,
            removed.objects,
            removed.purged,
            removed.deferred
        ),
        (1, 1, 1, 1, 0)
    );
    assert_erased(case, object).await;
    let repeated = tasks
        .maintain_checkpoints(&mut cursor)
        .await
        .expect("repeat collection");
    assert_eq!(
        repeated.purged, 0,
        "acknowledged quota is never released twice"
    );
    fixture.model.finish().await.expect("model cleanup");
    server.finish().await;
}

async fn expire(case: &Case, task: &str, plan: &afd_wire::task::CheckpointPlan, object: &str) {
    // Deterministically age this isolated test's actual capture, without sleeping
    // or forging a successful upload or terminal task result.
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("expiry fixture");
    sqlx::query("UPDATE fleet.task_object_pins SET expires_at = now() - interval '2 hours', operation_expires_at = now() - interval '2 hours' WHERE task_id = $1::uuid")
        .bind(task).execute(&mut *connection).await.expect("expired writer");
    sqlx::query("UPDATE fleet.task_checkpoints SET transfer_expires_at = now() - interval '2 hours' WHERE id = $1::uuid")
        .bind(&plan.checkpoint_id).execute(&mut *connection).await.expect("expired transfer");
    sqlx::query(
        "UPDATE fleet.task_bases SET updated_at = now() - interval '2 hours' WHERE id = $1::uuid",
    )
    .bind(&plan.base_id)
    .execute(&mut *connection)
    .await
    .expect("aged base");
    sqlx::query("UPDATE fleet.task_objects SET updated_at = now() - interval '2 hours', reclaim_after_at = now() - interval '2 hours' WHERE id = $1::uuid")
        .bind(object).execute(&mut *connection).await.expect("aged object");
    sqlx::query("UPDATE fleet.tasks SET checkpoint_cleanup_at = now() - interval '2 hours' WHERE id = $1::uuid")
        .bind(task).execute(&mut *connection).await.expect("due owner");
}

async fn assert_erased(case: &Case, object: &str) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("custody observer");
    let quota: (i64, i64) = sqlx::query_as("SELECT reserved_bytes, stored_bytes FROM fleet.task_storage_quota WHERE workspace_id = $1::uuid")
        .bind(case.scope.workspace_id.as_str()).fetch_one(&mut *connection).await.expect("quota");
    assert_eq!(quota, (0, 0));
    let custody: (bool, bool) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM fleet.task_objects WHERE id = $1::uuid), EXISTS (SELECT 1 FROM fleet.task_storage_purges WHERE task_object_id = $1::uuid AND completed_at IS NOT NULL)")
        .bind(object).fetch_one(&mut *connection).await.expect("erasure receipt");
    assert_eq!(
        custody,
        (false, true),
        "private key erased; opaque remote-delete receipt retained"
    );
    drop(connection);
}

async fn capture(
    tasks: &afd_fleet::task::Tasks,
    case: &Case,
    task: &afd_wire::task::DecisionTaskVersion,
) -> (
    afd_wire::task::CheckpointPlan,
    afd_wire::task::CheckpointObjectsPage,
) {
    let mut request = input(
        task,
        case,
        CheckpointMode::RetainBase,
        None,
        &"0".repeat(40),
    );
    request.upload_budget_bytes = 20;
    let plan = tasks
        .plan_checkpoint(case.lease(), &valid(request))
        .await
        .expect("capture");
    let objects = tasks
        .register_checkpoint_objects(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(CheckpointObjectsInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                pin_id: plan.pin.id.clone(),
                objects: vec![CheckpointPlaintext {
                    sha256: "a".repeat(64),
                    size: 4,
                    kind: "file".into(),
                }],
            }),
        )
        .await
        .expect("unuploaded allocated object");
    (plan, objects)
}
