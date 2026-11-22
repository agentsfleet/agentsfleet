//! Writer custody and private retention preserve the last committed turn and its public source.

use super::{Case, Checked, lifecycle::isolated, manifest::Pending, storage::Files};
use crate::support::Fixtures;
use af_core::{error_code, id::Uuid7};
use af_wire::run::{
    TurnCheckpointAbandon, TurnCheckpointAction as Action, TurnCheckpointPinRelease,
    TurnCheckpointRead, TurnCheckpointResponse as Response, TurnResume, TurnWorkspace,
};
use afd_fleet::durable_task::{CheckpointMaintenanceCursor, DurableTaskLifecycle as _};
use sqlx::Connection as _;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reading_an_uncaptured_assignment_does_not_allocate_recovery_state() -> Checked {
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
            .execute(Action::Abandon(TurnCheckpointAbandon {
                checkpoint: pending.reference.clone(),
            }))
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
            .execute(Action::PinRelease(TurnCheckpointPinRelease {
                checkpoint: pending.reference.clone(),
            }))
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
        sqlx::query_scalar("SELECT id FROM thread.turns WHERE id=$1 FOR NO KEY UPDATE")
            .bind(&older.case.acquired.turn_id)
            .fetch_one(&mut *held)
            .await?;
    assert_eq!(locked, older.case.acquired.turn_id);

    let tasks = newer.case.tasks();
    let mut cursor = CheckpointMaintenanceCursor::default();
    let captured = af_core::test_util::trace::Capture::install();
    let pass = tasks.maintain_checkpoints(&mut cursor).await;
    let deferred = captured.only("turn_checkpoint_retention_deferred");
    drop(captured);
    let pass = pass?;
    assert_eq!((pass.owners, pass.deferred), (1, 1));
    assert_eq!(
        deferred.field("turn_id"),
        Some(older.case.acquired.turn_id.as_str())
    );
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
         FROM runner.checkpoints p JOIN runner.checkpoint_pins pin
            ON pin.checkpoint_id=p.id
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
        .execute(Action::Read(TurnCheckpointRead {
            descriptor: Box::new(head.clone()),
        }))
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
         (SELECT count(*) FROM thread.turns WHERE id=$1 AND next_checkpoint_generation IS NOT NULL),
         (SELECT count(*) FROM runner.checkpoints WHERE turn_id=$1),
         (SELECT count(*) FROM thread.replay_receipts WHERE scope LIKE $2)",
    )
    .bind(&files.case.acquired.turn_id)
    .bind(super::receipts_of(&files.case.acquired.turn_id))
    .fetch_one(&mut *connection)
    .await?)
}

/// Admits `count` unsettled turns on `files`' fleet, each with an empty
/// recovery head, as a run's first Begin leaves one.
async fn idle_owners(files: &Files, count: usize) -> Checked<Vec<Uuid7>> {
    let turns = afd_turns::Turns::new(
        files.case.fixtures.database.clone(),
        files.case.fixtures.bus().clone(),
        af_crypto::entropy::Entropy::new(),
    );
    let actor = afd_turns::view::steer_actor("retention-owner");
    let body = serde_json::json!({"message": "Check the staging deploy"}).to_string();
    let mut connection = files.case.fixtures.database.acquire().await?;
    let mut owners = Vec::with_capacity(count);
    for _ in 0..count {
        let admitted = turns
            .admit(afd_turns::IncomingTurn {
                producer: afd_turns::Producer::Steer,
                key: afd_turns::Key::Unrepeatable,
                fleet: &files.case.acquired.fleet_id,
                workspace: &files.case.acquired.workspace_id,
                actor: &actor,
                event_type: af_wire::turn::TurnKind::Chat,
                request_json: &body,
                reply: afd_turns::Reply::None,
            })
            .await?;
        sqlx::query("UPDATE thread.turns SET next_checkpoint_generation = 1 WHERE id = $1")
            .bind(&admitted.stored.id)
            .execute(&mut *connection)
            .await?;
        owners.push(admitted.stored.id);
    }
    Ok(owners)
}

// Value: protects=a retention pass over a full page resumes after it, so an
// owner past the first hundred is collected; fails_when=the cursor resets after
// a full page as it does after a short one; why_new=the full-page arm had no
// case; seam=the production maintenance pass over 101 recovery heads, only the
// last in key order settled past retention.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn retention_resumes_after_a_full_page_of_owners() -> Checked {
    let files = isolated().await?;
    let owners = idle_owners(&files, 101).await?;
    let last = owners.iter().max().ok_or("owners")?.clone();
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE thread.turns SET settled_at=clock_timestamp()-interval '8 days', updated_at=now() WHERE id=$1")
        .bind(&last)
        .execute(&mut *connection)
        .await?;
    let head =
        "SELECT count(*) FROM thread.turns WHERE id=$1 AND next_checkpoint_generation IS NOT NULL";

    let tasks = files.case.tasks();
    let mut cursor = CheckpointMaintenanceCursor::default();
    let first = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!(first.deferred, 0);
    let kept: i64 = sqlx::query_scalar(head)
        .bind(&last)
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(kept, 1, "the last owner is past the first page");
    let second = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!(second.deferred, 0);
    let kept: i64 = sqlx::query_scalar(head)
        .bind(&last)
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(
        kept, 0,
        "the next pass resumes after the full page and collects it"
    );
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE next_checkpoint_generation IS NOT NULL",
    )
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(total, 100, "every unsettled owner keeps its head");
    drop(connection);
    drop(tasks);
    files.finish().await;
    Ok(())
}

/// Deletes `statement`'s row once the pass waits on `held`'s fleet lock,
/// then commits, so the pass resumes against an owner that is gone.
async fn vanish_while_waiting(
    files: &Files,
    mut held: sqlx::Transaction<'_, sqlx::Postgres>,
    blocker: i32,
    statement: &'static str,
    id: &Uuid7,
) -> Checked {
    let mut observer = files.case.fixtures.database.acquire().await?;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker)
            .fetch_one(&mut *observer)
            .await?;
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
        let gone = sqlx::query(statement).bind(id).execute(&mut *held).await?;
        assert_eq!(gone.rows_affected(), 1);
        held.commit().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    Ok(())
}

// Value: protects=a retention pass that waited on an owner whose recovery head
// or fleet went meanwhile collects nothing and fails nothing; fails_when=retain
// stops treating a vanished head or parent as nothing to collect; why_new=both
// skip arms had no case; seam=the production maintenance pass blocked on a
// held fleet row lock while the holder deletes the head, then the fleet.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn retention_skips_an_owner_that_went_while_it_waited() -> Checked {
    for fleet in [false, true] {
        let files = isolated().await?;
        committed(&files).await?;
        age_source(&files).await?;
        let (statement, id) = if fleet {
            (
                "DELETE FROM fleet.fleets WHERE id=$1",
                &files.case.acquired.fleet_id,
            )
        } else {
            (
                "UPDATE thread.turns SET head_checkpoint_id=NULL, next_checkpoint_generation=NULL WHERE id=$1",
                &files.case.acquired.turn_id,
            )
        };
        let mut connection = files.case.fixtures.database.acquire().await?;
        let mut held = connection.begin().await?;
        let blocker: i32 =
            sqlx::query_scalar("SELECT pg_backend_pid() FROM fleet.fleets WHERE id=$1 FOR UPDATE")
                .bind(&files.case.acquired.fleet_id)
                .fetch_one(&mut *held)
                .await?;
        let tasks = files.case.tasks();
        let mut cursor = CheckpointMaintenanceCursor::default();
        let (pass, vanished) = tokio::join!(
            tasks.maintain_checkpoints(&mut cursor),
            vanish_while_waiting(&files, held, blocker, statement, id),
        );
        vanished?;
        let pass = pass?;
        assert_eq!(
            (pass.owners, pass.deferred),
            (0, 0),
            "fleet deleted: {fleet}"
        );
        assert_eq!(private_rows(&files).await?.0, 0, "fleet deleted: {fleet}");
        drop(tasks);
        drop(connection);
        files.finish().await;
    }
    Ok(())
}
