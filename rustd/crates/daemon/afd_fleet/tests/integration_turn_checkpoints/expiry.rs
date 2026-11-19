//! Custody must remain live after a real manifest lock wait, with the run still live.

use super::{Checked, manifest::Pending, storage::Files};
use af_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;
use sqlx::{Connection as _, Postgres, Transaction};
use std::time::Duration;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn commit_rechecks_pin_and_plan_deadlines_after_manifest_locks() -> Checked {
    for plan_expires in [false, true] {
        expired_commit(plan_expires).await?;
    }
    Ok(())
}

async fn expired_commit(plan_expires: bool) -> Checked {
    let files = Files::new().await?;
    let pending = Pending::new(&files, &[Vec::new()]).await?;
    let id = pending
        .plan
        .page_ids
        .first()
        .ok_or("page identity")?
        .clone();
    let digest = pending.write(&files, id.clone(), 0, Vec::new()).await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    let deadline = shorten(&mut connection, &pending, plan_expires).await?;
    let mut held = connection.begin().await?;
    let blocker: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM runner.turn_checkpoint_pages WHERE id=$1 FOR UPDATE",
    )
    .bind(Uuid7::parse(&id.0)?)
    .fetch_one(&mut *held)
    .await?;
    let pages = [digest];
    let (verdict, released) = tokio::join!(
        pending.commit(&files, &pages),
        release_after_expiry(&files, held, blocker, deadline),
    );
    released?;
    assert!(verdict.is_err_and(|failure| {
        failure
            .downcast_ref::<afd_fleet::Error>()
            .is_some_and(|failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_CONFLICT)
    }));
    assert!(files.case.counts().await?.3.is_none());
    drop(connection);
    files.finish().await;
    Ok(())
}

async fn shorten(
    connection: &mut sqlx::PgConnection,
    pending: &Pending,
    plan_expires: bool,
) -> Checked<jiff::Timestamp> {
    let statement = if plan_expires {
        "UPDATE runner.turn_checkpoints SET expires_at=clock_timestamp()+interval '1400 milliseconds' WHERE id=$1 RETURNING expires_at"
    } else {
        "UPDATE runner.turn_checkpoint_pins SET expires_at=clock_timestamp()+interval '1400 milliseconds' WHERE id=$1 RETURNING expires_at"
    };
    let id = if plan_expires {
        &pending.plan.checkpoint_id.0
    } else {
        &pending.plan.pin.id
    };
    let deadline: jiff_sqlx::Timestamp = sqlx::query_scalar(statement)
        .bind(Uuid7::parse(id)?)
        .fetch_one(connection)
        .await?;
    Ok(deadline.to_jiff())
}

pub(super) async fn release_after_expiry(
    files: &Files,
    held: Transaction<'_, Postgres>,
    blocker: i32,
    deadline: jiff::Timestamp,
) -> Checked {
    let mut observer = files.case.fixtures.database.acquire().await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let (waiting, live): (bool, bool) = sqlx::query_as(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid))),clock_timestamp()<$2",
            ).bind(blocker).bind(deadline.to_sqlx()).fetch_one(&mut *observer).await?;
            assert!(live, "the operation must reach the held page before custody expires");
            if waiting { break; }
            tokio::task::yield_now().await;
        }
        loop {
            let expired: bool = sqlx::query_scalar("SELECT clock_timestamp()>=$1")
                .bind(deadline.to_sqlx()).fetch_one(&mut *observer).await?;
            if expired { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        held.rollback().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }).await??;
    Ok(())
}
