//! The fleet row lock bounds a send, and staging admits a fleet in any status.

use afd_db::test_util::mint_id;
use afd_turns::Reply as ReplyTo;
use sqlx::Connection as _;

use super::{Fallible, lane::Lane, ledger::steer, queue::over_lane};

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn staging_admits_on_an_archived_fleet() -> Fallible {
    let lane = Lane::seed().await?;
    let turns = over_lane(&lane).await?;
    let key = mint_id();
    let refused = turns
        .admit_runnable(steer(&lane, key.as_str(), ReplyTo::None))
        .await
        .err()
        .ok_or("an archived fleet takes no runnable send")?;
    assert_eq!(
        refused.not_runnable(),
        Some(afd_fleet_lifecycle::FleetStatus::Archived)
    );
    let staged_key = mint_id();
    let prepared = turns
        .prepare(steer(&lane, staged_key.as_str(), ReplyTo::None))
        .await?;
    let mut connection = lane.database.acquire().await?;
    let mut tx = connection.begin().await?;
    let staged = turns.stage(&mut tx, prepared).await?;
    tx.commit().await?;
    assert!(!staged.replayed);
    drop(connection);
    lane.cleanup().await;
    Ok(())
}

/// The SQLSTATE a lock wait past `lock_timeout` raises.
const LOCK_NOT_AVAILABLE: &str = "55P03";

/// Whether `55P03` sits anywhere in the error's cause chain.
fn timed_out_on_a_lock(error: &afd_turns::Error) -> bool {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = cause {
        if let Some(sql) = current.downcast_ref::<sqlx::Error>()
            && sql
                .as_database_error()
                .and_then(sqlx::error::DatabaseError::code)
                .as_deref()
                == Some(LOCK_NOT_AVAILABLE)
        {
            return true;
        }
        cause = current.source();
    }
    false
}

/// A send behind a fleet row held past `LOCK_TIMEOUT` fails alone, retryable,
/// and records nothing.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_send_behind_a_held_fleet_row_times_out() -> Fallible {
    let lane = Lane::seed().await?;
    let turns = over_lane(&lane).await?;
    let mut connection = lane.database.acquire().await?;
    let mut holder = connection.begin().await?;
    sqlx::query("SELECT 1 FROM fleet.fleets WHERE id = $1 FOR UPDATE")
        .bind(&lane.fleet)
        .execute(&mut *holder)
        .await?;
    let key = mint_id();
    let refused = turns
        .admit(steer(&lane, key.as_str(), ReplyTo::None))
        .await
        .err()
        .ok_or("the send gives up after the lock bound")?;
    assert!(refused.is_datastore_unavailable(), "{refused}");
    assert!(
        !refused.is_over_capacity(),
        "a timeout is not capacity: {refused}"
    );
    assert_eq!(refused.code(), af_core::error_code::INTERNAL_DB_UNAVAILABLE);
    assert!(timed_out_on_a_lock(&refused), "{refused}");
    holder.rollback().await?;
    drop(connection);
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE producer IS NOT NULL AND thread_id = $1",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.database.acquire().await?)
    .await?;
    assert_eq!(rows, 0, "a timed-out send records nothing");
    lane.cleanup().await;
    Ok(())
}

/// A archived fleet's refusal is the caller's answer, not a turn failure:
/// no `turn_failed` and no capacity line, and so no tick of
/// `agentsfleet_turns_total`, which `count_refusal` records beside that
/// log.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_archived_fleet_refusal_counts_no_turn() -> Fallible {
    let capture = af_core::test_util::trace::Capture::install();
    let lane = Lane::seed().await?;
    let turns = over_lane(&lane).await?;
    let key = mint_id();
    let refused = turns
        .admit_runnable(steer(&lane, key.as_str(), ReplyTo::None))
        .await
        .err()
        .ok_or("the lane's fleet is archived")?;
    assert_eq!(
        refused.not_runnable(),
        Some(afd_fleet_lifecycle::FleetStatus::Archived)
    );
    let refusal_lines = capture
        .events()
        .into_iter()
        .filter(|event| {
            matches!(
                event.field("event"),
                Some("turn_refused" | "turn_refused_over_capacity")
            )
        })
        .count();
    assert_eq!(
        refusal_lines, 0,
        "an archived-fleet refusal is not a turn failure"
    );
    lane.cleanup().await;
    Ok(())
}
