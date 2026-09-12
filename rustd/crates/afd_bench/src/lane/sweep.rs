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
use afd_redis::{FleetStreams, OUTBOUND_STREAM_KEY, ReadyIndex, Redis};
use sqlx::Row as _;

use crate::datastores::command::{EXISTS, RANGE_END, RANGE_START, XDEL, XGROUP, XLEN, XRANGE};
use crate::error::{Error, Result};
use crate::fixture::RunPrefix;

/// Matches every name a run prefixed.
fn like(prefix: &RunPrefix) -> String {
    format!("{}-%", prefix.as_str())
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
    sqlx::query("SELECT id::text FROM core.fleets WHERE name LIKE $1")
        .bind(pattern)
        .fetch_all(&mut *connection)
        .await?
        .iter()
        .map(|row| row.try_get::<String, _>(0).map_err(Error::from))
        .collect()
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

/// Delete the outbound entries this run appended, by the ids it was handed.
///
/// The happy path: a lane holds every id `enqueue` returned, so it removes
/// exactly those and never reads the shared stream to find them. Returns how
/// many the server actually removed, which is what the ledger compares.
///
/// # Errors
///
/// [`crate::Error::QueueUnavailable`] when the stream will not answer.
pub async fn outbound_entries(queue: &Redis, ids: &[String]) -> Result<u64> {
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
/// [`crate::Error::QueueUnavailable`] when the stream will not answer.
pub async fn outbound_stream(queue: &Redis, prefix: &RunPrefix) -> Result<u64> {
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
    let removed = outbound_entries(queue, &mine).await?;
    // Destroy only this run's group. The daemon group and its PEL are never
    // consulted or modified, even if a foreign entry arrived during the run.
    let mut exists = redis::cmd(EXISTS);
    exists.arg(OUTBOUND_STREAM_KEY);
    if queue
        .command::<u64>(EXISTS, OUTBOUND_STREAM_KEY, &exists)
        .await?
        == 0
    {
        return Ok(removed);
    }
    let mut destroy = redis::cmd(XGROUP);
    destroy
        .arg("DESTROY")
        .arg(OUTBOUND_STREAM_KEY)
        .arg(prefix.name("outbound-consumer-group"));
    let groups: u64 = queue.command(XGROUP, OUTBOUND_STREAM_KEY, &destroy).await?;
    Ok(removed + groups)
}

/// The entry field the outbound producer writes the workspace into.
const WORKSPACE_FIELD: &str = "workspace_id";

/// Refuse to attach the deployment-wide outbound consumer to existing work.
///
/// The queue has one stream and group. A synthetic worker cannot distinguish
/// another producer's entry before consuming it, so saturation requires an
/// empty repository-owned stream.
///
/// # Errors
///
/// [`Error::SharedTargetState`] when entries already exist, or the queue's
/// error when it cannot read the stream length.
pub async fn require_outbound_empty(queue: &Redis) -> Result<()> {
    let mut length = redis::cmd(XLEN);
    length.arg(OUTBOUND_STREAM_KEY);
    let entries: u64 = queue.command(XLEN, OUTBOUND_STREAM_KEY, &length).await?;
    if entries > 0 {
        return Err(Error::SharedTargetState { entries });
    }
    Ok(())
}
