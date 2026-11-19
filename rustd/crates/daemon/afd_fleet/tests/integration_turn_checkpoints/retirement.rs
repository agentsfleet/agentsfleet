//! Writer custody and private retention preserve the last committed turn and its public source.

use super::{Case, Checked, lifecycle::isolated, manifest::Pending, storage::Files};
use crate::support::Fixtures;
use af_core::{error_code, id::Uuid7};
use af_wire::run::{
    TurnCheckpointAction as Action, TurnCheckpointResponse as Response, TurnResume, TurnWorkspace,
};
use afd_fleet::task::{CheckpointMaintenanceCursor, TaskLifecycle as _};
use sqlx::Connection as _;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reading_an_uncaptured_offer_does_not_allocate_recovery_state() -> Checked {
    let files = isolated().await?;
    assert!(
        files
            .case
            .runs
            .turn_resume(files.case.run())
            .await?
            .is_none()
    );
    assert_eq!(private_rows(&files).await?, (0, 0, 0));
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_api_only_abandon_releases_its_writer_and_keeps_the_previous_head() -> Checked {
    let files = isolated().await?;
    let head = committed(&files).await?;
    let pending = Pending::allocate(
        &files,
        &[],
        TurnWorkspace::ApiOnly,
        Some(head.checkpoint_id.clone()),
    )
    .await?;
    assert_eq!(writer_state(&files, &pending).await?, (false, false, false));
    assert!(matches!(
        files
            .execute(Action::Abandon {
                checkpoint: pending.reference.clone(),
            })
            .await?,
        Response::Abandoned
    ));
    assert_eq!(writer_state(&files, &pending).await?, (false, true, true));
    assert_preserved_head_and_refused_commit(&files, &pending, &head).await?;
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn releasing_a_writer_pin_preserves_both_the_pending_plan_and_the_previous_head() -> Checked {
    let files = isolated().await?;
    let head = committed(&files).await?;
    let pending = Pending::allocate(
        &files,
        &[],
        TurnWorkspace::ApiOnly,
        Some(head.checkpoint_id.clone()),
    )
    .await?;
    assert_eq!(writer_state(&files, &pending).await?, (false, false, false));
    assert!(matches!(
        files
            .execute(Action::PinRelease {
                checkpoint: pending.reference.clone(),
            })
            .await?,
        Response::PinReleased
    ));
    assert_eq!(writer_state(&files, &pending).await?, (false, false, true));
    assert_preserved_head_and_refused_commit(&files, &pending, &head).await?;
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn terminal_api_only_retention_erases_private_rows_and_preserves_the_source() -> Checked {
    let files = isolated().await?;
    let head = committed(&files).await?;
    let (plans, operations, _, saved) = files.case.counts().await?;
    assert_eq!(plans, 1);
    assert!(operations > 0);
    assert_eq!(saved, Some(Uuid7::parse(&head.checkpoint_id.0)?));
    age_source(&files).await?;
    let before = public_source(&files).await?;

    let tasks = files.case.tasks();
    let mut cursor = CheckpointMaintenanceCursor::default();
    assert_eq!(tasks.maintain_checkpoints(&mut cursor).await?.owners, 1);
    assert_eq!(private_rows(&files).await?, (0, 0, 0));
    assert_eq!(public_source(&files).await?, before);
    assert_eq!(tasks.maintain_checkpoints(&mut cursor).await?.owners, 0);
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn retention_collects_a_second_owner_while_the_first_head_is_locked() -> Checked {
    let first = isolated().await?;
    let fixtures = Fixtures::create_in(first.case.fixtures.shared_lane()).await;
    let second = Files::for_case(Case::with_fixtures(fixtures).await?).await?;
    for files in [&first, &second] {
        committed(files).await?;
        age_source(files).await?;
    }
    let (older, newer) = if first.case.acquired.turn_id < second.case.acquired.turn_id {
        (&first, &second)
    } else {
        (&second, &first)
    };
    let mut connection = older.case.fixtures.database.acquire().await?;
    let mut held = connection.begin().await?;
    let locked: Uuid7 =
        sqlx::query_scalar("SELECT turn_id FROM runner.turn_recovery WHERE turn_id=$1 FOR UPDATE")
            .bind(&older.case.acquired.turn_id)
            .fetch_one(&mut *held)
            .await?;
    assert_eq!(locked, older.case.acquired.turn_id);

    let tasks = newer.case.tasks();
    let mut cursor = CheckpointMaintenanceCursor::default();
    let pass = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!((pass.owners, pass.deferred), (1, 1));
    assert_eq!(private_rows(newer).await?, (0, 0, 0));
    assert_eq!(private_rows(older).await?, (1, 1, 1));
    held.rollback().await?;
    drop(connection);
    let pass = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!((pass.owners, pass.deferred), (1, 0));
    assert_eq!(private_rows(older).await?, (0, 0, 0));
    drop(tasks);
    second.finish().await;
    first.finish().await;
    Ok(())
}

async fn committed(files: &Files) -> Checked<Box<TurnResume>> {
    let pending = Pending::allocate(files, &[], TurnWorkspace::ApiOnly, None).await?;
    let Response::Committed(head) = pending.commit(files, &[]).await? else {
        return Err("expected committed API-only generation".into());
    };
    Ok(head)
}

async fn writer_state(files: &Files, pending: &Pending) -> Checked<(bool, bool, bool)> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    Ok(sqlx::query_as(
        "SELECT p.ready_at IS NOT NULL, p.abandoned_at IS NOT NULL, pin.released_at IS NOT NULL
         FROM runner.turn_checkpoints p JOIN runner.turn_checkpoint_pins pin
            ON pin.turn_checkpoint_id=p.id
         WHERE p.id=$1 AND pin.id=$2",
    )
    .bind(Uuid7::parse(&pending.plan.checkpoint_id.0)?)
    .bind(Uuid7::parse(&pending.plan.pin.id)?)
    .fetch_one(&mut *connection)
    .await?)
}

async fn assert_preserved_head_and_refused_commit(
    files: &Files,
    pending: &Pending,
    head: &TurnResume,
) -> Checked {
    let failure = pending
        .commit(files, &[])
        .await
        .err()
        .ok_or("commit succeeded after the writer pin was released")?;
    let failure = failure
        .downcast_ref::<afd_fleet::Error>()
        .ok_or("expected a typed checkpoint refusal")?;
    assert_eq!(failure.code(), error_code::RUN_CHECKPOINT_CONFLICT);
    let Response::Read(read) = files
        .execute(Action::Read {
            descriptor: Box::new(head.clone()),
        })
        .await?
    else {
        return Err("expected the previous ready generation".into());
    };
    assert_eq!(&read.descriptor, head);
    assert_eq!(
        files.case.counts().await?.3,
        Some(Uuid7::parse(&head.checkpoint_id.0)?)
    );
    Ok(())
}

async fn age_source(files: &Files) -> Checked {
    let mut connection = files.case.fixtures.database.acquire().await?;
    assert_eq!(
        sqlx::query(
            "UPDATE thread.turns
             SET settled_at=clock_timestamp()-interval '8 days', updated_at=now() WHERE id=$1",
        )
        .bind(&files.case.acquired.turn_id)
        .execute(&mut *connection)
        .await?
        .rows_affected(),
        1
    );
    Ok(())
}

async fn public_source(files: &Files) -> Checked<String> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(t)::text FROM thread.turns t WHERE t.id=$1 AND t.opened_at IS NOT NULL",
    )
    .bind(&files.case.acquired.turn_id)
    .fetch_one(&mut *connection)
    .await?)
}

async fn private_rows(files: &Files) -> Checked<(i64, i64, i64)> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    Ok(sqlx::query_as(
        "SELECT
         (SELECT count(*) FROM runner.turn_recovery WHERE turn_id=$1),
         (SELECT count(*) FROM runner.turn_checkpoints WHERE turn_id=$1),
         (SELECT count(*) FROM runner.turn_checkpoint_operations WHERE turn_id=$1)",
    )
    .bind(&files.case.acquired.turn_id)
    .fetch_one(&mut *connection)
    .await?)
}
