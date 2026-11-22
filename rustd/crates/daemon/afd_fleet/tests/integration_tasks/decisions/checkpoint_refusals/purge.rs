//! A purge recorded under another bucket waits; one whose workspace is gone still
//! completes remotely, with no quota left to release.

use af_core::{error_code, etag::sha256_hex, id::Uuid7, test_util::trace::Capture};
use af_wire::task::{CheckpointMode, CheckpointObjectsInput, CheckpointPlaintext};
use afd_db::test_util::mint_id;
use afd_fleet::task::{CheckpointMaintenanceCursor, TaskLifecycle as _};

use super::{
    super::{
        super::fixture::{Case, exercise, id},
        checkpoint::{input, valid},
        checkpoint_remote::configured,
        checkpoint_server::Server,
        fixture::Decisions,
    },
    connection, planned,
};

/// What the purge pass logs for an attempt it leaves for later.
const EVENT_DEFERRED: &str = "dt_storage_purge_deferred";

// Value: protects=a purge never deletes from a bucket it was not recorded in, and a
// workspace's deletion never strands its remote bytes; fails_when=the bucket check is
// dropped from the purge pass, a deferred attempt is counted or not logged, or the
// acknowledgement requires the workspace row; why_new=the foreign-bucket and
// deleted-workspace purge arms had no case; seam=the production maintenance pass over
// live Postgres and real SDK deletes to the private HTTPS fixture, purge custody seeded
// in its real table.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_foreign_bucket_purge_waits_and_a_deleted_workspaces_purge_still_completes() {
    exercise(purges).await;
}

#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn purges(case: &Case) {
    let server = Server::start().await;
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = configured(&fixture, &server).await;
    let mut request = input(
        &task,
        case,
        CheckpointMode::RetainBase,
        None,
        &"0".repeat(40),
    );
    request.upload_budget_bytes = 64;
    let plan = planned(&tasks, case, request).await;
    tasks
        .register_checkpoint_objects(
            case.run(),
            &id(&plan.checkpoint_id),
            &valid(CheckpointObjectsInput {
                fencing_token: case.run().fencing_token,
                task: task.clone(),
                pin_id: plan.pin.id.clone(),
                objects: vec![CheckpointPlaintext {
                    sha256: sha256_hex(b"live"),
                    size: 4,
                    kind: "file".into(),
                }],
            }),
        )
        .await
        .expect("a live object records this bucket's reference");
    let bucket: String = sqlx::query_scalar(
        "SELECT bucket_reference FROM fleet.task_objects WHERE workspace_id = $1",
    )
    .bind(&case.scope.workspace_id)
    .fetch_one(&mut *connection(case).await)
    .await
    .expect("this storage's bucket reference");
    let before = quota(case).await;

    let foreign = mint_id();
    seed(case, &foreign, &case.scope.workspace_id, "another-bucket").await;
    let orphaned = mint_id();
    seed(case, &orphaned, &mint_id(), &bucket).await;

    let captured = Capture::install();
    let pass = tasks
        .maintain_checkpoints(&mut CheckpointMaintenanceCursor::default())
        .await;
    let deferred = captured.only(EVENT_DEFERRED);
    drop(captured);

    let pass = pass.expect("one maintenance pass");
    assert_eq!((pass.purged, pass.deferred), (1, 1));
    assert_eq!(deferred.field("object_id"), Some(foreign.as_str()));
    assert_eq!(
        deferred.field("error_code"),
        Some(error_code::TASK_STORAGE_UNAVAILABLE.as_str())
    );
    assert_eq!(
        completed(case, &orphaned).await,
        (true, true),
        "the deleted workspace's purge completes"
    );
    assert_eq!(
        completed(case, &foreign).await,
        (false, false),
        "the foreign purge waits, rescheduled for a later pass"
    );
    assert_eq!(
        quota(case).await,
        before,
        "neither purge moved this workspace's quota"
    );
    fixture.model.finish().await.expect("fixture cleanup");
    server.finish().await;
}

/// Seeds one due purge, as an object's deletion records it.
async fn seed(case: &Case, object: &Uuid7, workspace: &Uuid7, bucket: &str) {
    sqlx::query(
        "INSERT INTO fleet.task_storage_purges (task_object_id, workspace_id, task_identity_key_id, bucket_reference, accounted_bytes, stored, next_attempt_at, created_at, updated_at) VALUES ($1, $2, $3, $4, 20, true, now() - interval '1 minute', now(), now())",
    )
    .bind(object)
    .bind(workspace)
    .bind(mint_id())
    .bind(bucket)
    .execute(&mut *connection(case).await)
    .await
    .expect("a due purge");
}

/// Whether `object`'s purge completed, and whether it is still due now.
async fn completed(case: &Case, object: &Uuid7) -> (bool, bool) {
    let (done, due): (bool, bool) = sqlx::query_as(
        "SELECT completed_at IS NOT NULL, next_attempt_at <= now() FROM fleet.task_storage_purges WHERE task_object_id = $1",
    )
    .bind(object)
    .fetch_one(&mut *connection(case).await)
    .await
    .expect("the purge row");
    (done, done || due)
}

async fn quota(case: &Case) -> (i64, i64) {
    sqlx::query_as(
        "SELECT reserved_bytes, stored_bytes FROM fleet.task_storage_quota WHERE workspace_id = $1",
    )
    .bind(&case.scope.workspace_id)
    .fetch_one(&mut *connection(case).await)
    .await
    .expect("this workspace's quota")
}
