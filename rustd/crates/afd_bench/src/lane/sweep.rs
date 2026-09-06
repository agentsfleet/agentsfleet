//! Removing everything a run created, and reporting how much that was.
//!
//! # Prefix-scoped and idempotent
//!
//! The sweep deletes by the run prefix in each row's `name`, so it removes this
//! run's objects and nothing else. Running it twice is harmless, and running it
//! against a prefix an earlier run abandoned is how an orphan gets collected —
//! which is why a lane sweeps on every exit path rather than only the happy
//! one.
//!
//! # Redis first, then Postgres, then the parents
//!
//! The fleet ids come from Postgres, so the Redis half has to happen while the
//! rows still exist. After that the order is forced by the foreign keys:
//! fleets reference workspaces, workspaces reference tenants.

use afd_db::Db;
use afd_redis::{FleetStreams, ReadyIndex, Redis};
use sqlx::Row as _;

use crate::error::Result;
use crate::fixture::RunPrefix;

/// Matches every name a run prefixed.
fn like(prefix: &RunPrefix) -> String {
    format!("{}%", prefix.as_str())
}

/// Remove everything carrying this run's prefix, returning how many rows went.
///
/// # Errors
///
/// Whatever Postgres or Redis refused. A sweep that cannot finish is reported
/// rather than swallowed: the whole point of the count in the result file is
/// that somebody can see it did not match.
pub async fn everything(database: &Db, queue: &Redis, prefix: &RunPrefix) -> Result<u64> {
    let pattern = like(prefix);
    let fleets = fleet_ids(database, &pattern).await?;
    forget_streams(queue, &fleets).await?;
    rows(database, &pattern).await
}

/// The fleet ids this run created, read before the rows go.
async fn fleet_ids(database: &Db, pattern: &str) -> Result<Vec<String>> {
    let mut connection = database.acquire().await?;
    Ok(
        sqlx::query("SELECT id::text FROM core.fleets WHERE name LIKE $1")
            .bind(pattern)
            .fetch_all(&mut *connection)
            .await?
            .iter()
            .filter_map(|row| row.try_get::<String, _>(0).ok())
            .collect(),
    )
}

/// Drop each fleet's stream and its readiness mark.
///
/// Redis has no database-per-run equivalent: the readiness index is one hash at
/// a fixed key and a stream is keyed by fleet, so this is the only isolation
/// there is. A mark left behind would make the next run's first poll examine a
/// fleet whose rows are gone.
async fn forget_streams(queue: &Redis, fleets: &[String]) -> Result<()> {
    let streams = FleetStreams::new(queue.clone());
    let ready = ReadyIndex::new(queue.clone());
    for fleet in fleets {
        ready.force_clear(fleet).await?;
        streams.forget(fleet).await?;
    }
    Ok(())
}

/// Delete the rows, parents last, counting everything removed.
async fn rows(database: &Db, pattern: &str) -> Result<u64> {
    let mut connection = database.acquire().await?;
    let mut removed = 0;
    // Runners carry the prefix in `host_id` rather than a name: enrolment owns
    // that row's shape and there is no name column to put it in.
    for statement in [
        "DELETE FROM fleet.runners WHERE host_id LIKE $1",
        "DELETE FROM core.fleets WHERE name LIKE $1",
        "DELETE FROM core.workspaces WHERE name LIKE $1",
        "DELETE FROM core.tenants WHERE name LIKE $1",
    ] {
        removed += sqlx::query(statement)
            .bind(pattern)
            .execute(&mut *connection)
            .await?
            .rows_affected();
    }
    Ok(removed)
}
