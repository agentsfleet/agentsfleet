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
//! # Dragonfly first, then Postgres, then the parents
//!
//! The fleet ids come from Postgres, so the Dragonfly half has to happen while the
//! rows still exist. After that the order is forced by the foreign keys:
//! fleets reference workspaces, workspaces reference tenants.

use afd_db::Db;
use afd_dragonfly::{Dragonfly, OUTBOUND_STREAM_KEY};

use crate::datastores::command::{RANGE_END, RANGE_START, XDEL, XRANGE};
use crate::error::Result;
use crate::fixture::RunPrefix;

/// A run's runners, by the prefix in their host id.
const DELETE_RUNNERS: &str = include_str!("sql/delete_runners.sql");

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
/// Whatever Postgres or Dragonfly refused. A sweep that cannot finish is reported
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

/// Delete the outbound entries this run appended, by the ids it was handed.
///
/// The happy path: a lane holds every id `enqueue` returned, so it removes
/// exactly those and never reads the shared stream to find them. Returns how
/// many the server actually removed, which is what the ledger compares.
///
/// # Errors
///
/// `QueueUnavailable` when the stream will not answer.
pub async fn outbound_entries(queue: &Dragonfly, ids: &[String]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut del = redis::cmd(XDEL);
    del.arg(OUTBOUND_STREAM_KEY);
    for id in ids {
        del.arg(id);
    }
    Ok(queue.command(XDEL, OUTBOUND_STREAM_KEY, &del).await?)
}

/// Remove this run's entries from the shared outbound stream by prefix.
///
/// The fallback for a run that failed before it could hand its ids to
/// [`outbound_entries`]: the entries are found by the run prefix they carry
/// in `workspace_id`. This reads the stream to do it, which on the rig is the
/// run's own entries and little else; the paged, deployed-safe form of this
/// scan is deferred with the deployed-environment follow-up.
///
/// # Errors
///
/// `QueueUnavailable` when the stream will not answer.
pub async fn outbound_stream(queue: &Dragonfly, prefix: &RunPrefix) -> Result<u64> {
    let mut range = redis::cmd(XRANGE);
    range
        .arg(OUTBOUND_STREAM_KEY)
        .arg(RANGE_START)
        .arg(RANGE_END);
    let entries: Vec<(String, Vec<String>)> =
        queue.command(XRANGE, OUTBOUND_STREAM_KEY, &range).await?;
    let mine: Vec<String> = entries
        .into_iter()
        .filter(|(_id, fields)| {
            fields.chunks(2).any(|pair| {
                pair.first().is_some_and(|k| k == WORKSPACE_FIELD)
                    && pair.get(1).is_some_and(|v| prefix.owns(v))
            })
        })
        .map(|(id, _fields)| id)
        .collect();
    outbound_entries(queue, &mine).await
}

/// The entry field the outbound producer writes the workspace into.
const WORKSPACE_FIELD: &str = "workspace_id";
