//! Cleanup rechecks references after quota waits and releases physical quota once.

use super::{Case, Checked, files, storage::Files};
use crate::support::Fixtures;
use af_core::id::Uuid7;
use af_wire::{
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointRef,
        TurnCheckpointResponse as Response,
    },
    task::{CheckpointObjectStatus, CheckpointPlaintext},
};
use afd_fleet::task::{CheckpointMaintenanceCursor, TaskDependencies, TaskLifecycle as _, Tasks};
use sqlx::{Connection as _, Postgres, Transaction};
use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn turn_references_survive_an_orphan_snapshot_and_release_shared_quota_only_after_retention()
-> Checked {
    let files =
        Files::for_case(Case::with_fixtures(Fixtures::create_isolated().await).await?).await?;
    let object = orphan(&files).await?;
    let (plan, history) = files::begin(&files).await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(plan.pin.id.clone()),
    };
    let tasks = tasks(&files);
    race(&files, &tasks, &object, &reference).await?;
    files::publish(&files, plan, history, reference, object.as_str()).await?;
    age_object(&files, &object).await?;
    let live = tasks
        .maintain_checkpoints(&mut CheckpointMaintenanceCursor::default())
        .await?;
    assert_eq!(
        (live.objects, live.purged),
        (0, 0),
        "a ready turn reference protects old physical content"
    );
    assert_storage(&files, &object, true).await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE thread.turns SET settled_at=clock_timestamp()-interval '8 days',updated_at=now() WHERE id=$1")
        .bind(&files.case.acquired.turn_id).execute(&mut *connection).await?;
    drop(connection);
    let mut cursor = CheckpointMaintenanceCursor::default();
    let removed = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!((removed.owners, removed.objects, removed.purged), (1, 1, 1));
    assert_storage(&files, &object, false).await?;
    assert_eq!(tasks.maintain_checkpoints(&mut cursor).await?.purged, 0);
    files.finish().await;
    Ok(())
}

fn tasks(files: &Files) -> Tasks {
    let turns = afd_turns::Turns::new(
        files.case.fixtures.database.clone(),
        files.case.fixtures.bus().clone(),
        af_crypto::entropy::Entropy::new(),
    );
    let mut dependencies = TaskDependencies::from(&files.case.fixtures.plane());
    dependencies.storage = Some(files.storage.clone());
    Tasks::new(files.case.runs.clone(), turns, dependencies)
}

async fn orphan(files: &Files) -> Checked<Uuid7> {
    let (plan, _) = files::begin(files).await?;
    let reference = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id,
        pin_id: CheckpointOperationId(plan.pin.id),
    };
    let object = files::authenticated_chunk(files, &reference).await?;
    files
        .execute(Action::Abandon {
            checkpoint: reference,
        })
        .await?;
    files
        .case
        .tasks()
        .maintain_checkpoints(&mut CheckpointMaintenanceCursor::default())
        .await?;
    let id = Uuid7::parse(&object.object_id)?;
    age_object(files, &id).await?;
    Ok(id)
}

async fn age_object(files: &Files, object: &Uuid7) -> Checked {
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE fleet.task_objects SET updated_at=clock_timestamp()-interval '2 hours',reclaim_after_at=clock_timestamp()-interval '2 hours' WHERE id=$1")
        .bind(object).execute(&mut *connection).await?;
    Ok(())
}

async fn race(
    files: &Files,
    tasks: &Tasks,
    object: &Uuid7,
    reference: &TurnCheckpointRef,
) -> Checked {
    let mut connection = files.case.fixtures.database.acquire().await?;
    let mut held = connection.begin().await?;
    let controller: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM fleet.task_objects WHERE id=$1 FOR UPDATE",
    )
    .bind(object)
    .fetch_one(&mut *held)
    .await?;
    let (ready, start) = oneshot::channel();
    let registration = files.execute(Action::Objects {
        checkpoint: reference.clone(),
        objects: vec![CheckpointPlaintext {
            sha256: af_core::etag::sha256_hex(files::CONTENT),
            size: u64::try_from(files::CONTENT.len())?,
            kind: "file".to_owned(),
        }],
    });
    let collecting = async {
        start.await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
            tasks
                .maintain_checkpoints(&mut CheckpointMaintenanceCursor::default())
                .await?,
        )
    };
    let (registered, collected, released) = tokio::join!(
        registration,
        collecting,
        release_after_both_waits(files, held, controller, ready)
    );
    released?;
    let Response::Objects(objects) = registered? else {
        return Err("registered reused content".into());
    };
    assert_eq!(objects.len(), 1);
    let reused = objects.first().ok_or("reused object")?;
    assert_eq!(reused.object_id, object.as_str());
    assert!(matches!(reused.status, CheckpointObjectStatus::Reused));
    let collected: afd_fleet::task::CheckpointMaintenance = collected?;
    assert_eq!((collected.objects, collected.purged), (0, 0));
    assert_storage(files, object, true).await
}

async fn release_after_both_waits(
    files: &Files,
    held: Transaction<'_, Postgres>,
    controller: i32,
    ready: oneshot::Sender<()>,
) -> Checked {
    let mut observer = files.case.fixtures.database.acquire().await?;
    let registration = waiting(&mut observer, controller).await?;
    ready.send(()).map_err(|()| "collector disappeared")?;
    // Registration holds quota while waiting on the object. Cleanup selected the
    // orphan before waiting behind that same quota owner, so its snapshot is old.
    waiting(&mut observer, registration).await?;
    held.commit().await?;
    Ok(())
}

async fn waiting(connection: &mut sqlx::PgConnection, blocker: i32) -> Checked<i32> {
    Ok(tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let pid: Option<i32> = sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) ORDER BY pid LIMIT 1")
                .bind(blocker).fetch_optional(&mut *connection).await?;
            if let Some(pid) = pid { return Ok::<_, sqlx::Error>(pid); }
            tokio::task::yield_now().await;
        }
    }).await??)
}

async fn assert_storage(files: &Files, object: &Uuid7, live: bool) -> Checked {
    let mut connection = files.case.fixtures.database.acquire().await?;
    let (exists, pending, bytes): (bool, bool, i64) = sqlx::query_as("SELECT
        EXISTS (SELECT 1 FROM fleet.task_objects WHERE id=$1),
        EXISTS (SELECT 1 FROM fleet.task_storage_purges WHERE task_object_id=$1 AND completed_at IS NULL),
        (SELECT stored_bytes+reserved_bytes FROM fleet.task_storage_quota WHERE workspace_id=$2)")
        .bind(object).bind(&files.case.acquired.workspace_id).fetch_one(&mut *connection).await?;
    assert_eq!(exists, live);
    assert!(!pending);
    assert_eq!(
        bytes,
        if live {
            i64::try_from(files::CONTENT.len() + af_crypto::TAG_LEN)?
        } else {
            0
        }
    );
    Ok(())
}
