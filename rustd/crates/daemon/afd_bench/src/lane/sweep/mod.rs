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
//! # Children first, then the parents
//!
//! The order is forced by the foreign keys: fleets reference workspaces,
//! workspaces reference tenants.

use af_core::id::Uuid7;
use afd_db::Db;

use crate::error::Result;
use crate::fixture::RunPrefix;

/// A run's runners, by the prefix in their host id.
const DELETE_RUNNERS: &str = include_str!("sql/delete_runners.sql");

/// A run's owed answers, by row id.
const DELETE_OBLIGATIONS: &str = include_str!("sql/delete_obligations.sql");

/// A run's fleets; their revisions, events and slots go with them.
const DELETE_FLEETS: &str = include_str!("sql/delete_fleets.sql");

/// A run's setup chat attempts, which hold no key to the workspace (money
/// outlives it), so no cascade removes them.
const DELETE_ATTEMPTS: &str = include_str!("sql/delete_attempts.sql");

/// A run's workspaces; their sealed secrets and drafts go with them.
const DELETE_WORKSPACES: &str = include_str!("sql/delete_workspaces.sql");

/// A run's tenants; their wallets, owners and holds go with them.
const DELETE_TENANTS: &str = include_str!("sql/delete_tenants.sql");

/// Matches every name a run prefixed.
fn like(prefix: &RunPrefix) -> String {
    format!("{}%", prefix.as_str())
}

/// Remove everything carrying this run's prefix, returning how many rows went.
///
/// # Errors
///
/// Whatever Postgres refused. A sweep that cannot finish is reported
/// rather than swallowed: the whole point of the count in the result file is
/// that somebody can see it did not match.
pub async fn everything(database: &Db, prefix: &RunPrefix) -> Result<u64> {
    let pattern = like(prefix);
    rows(database, &pattern).await
}

/// Delete the rows, parents last, counting everything removed.
async fn rows(database: &Db, pattern: &str) -> Result<u64> {
    let mut connection = database.acquire().await?;
    let mut removed = 0;
    // Runners carry the prefix in `host_id` rather than a name: enrolment owns
    // that row's shape and there is no name column to put it in.
    for statement in [
        DELETE_RUNNERS,
        DELETE_FLEETS,
        DELETE_ATTEMPTS,
        DELETE_WORKSPACES,
        DELETE_TENANTS,
    ] {
        removed += sqlx::query(statement)
            .bind(pattern)
            .execute(&mut *connection)
            .await?
            .rows_affected();
    }
    Ok(removed)
}

/// Remove this run's owed answers by the row ids the lane handed out.
///
/// The happy path: the lane holds every id it owed, so it removes exactly
/// those. A run that failed before it could is covered by [`everything`]:
/// deleting the run's fleets cascades to their obligations.
///
/// # Errors
///
/// Whatever Postgres refused.
pub async fn outbound_obligations(database: &Db, ids: &[Uuid7]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut connection = database.acquire().await?;
    let removed = sqlx::query(DELETE_OBLIGATIONS)
        .bind(ids)
        .execute(&mut *connection)
        .await?;
    Ok(removed.rows_affected())
}
