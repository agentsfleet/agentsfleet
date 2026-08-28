//! Keeping history from growing without end.
//!
//! Three passes over two tables: settled leases past the window are deleted,
//! per-work events past it are deleted, and `active` leases nothing will ever
//! settle are flipped to `expired` so they stop looking like live work.
//!
//! # Why an age-keyed sweep can never reach live work
//!
//! A lease's total wall clock is capped: renewal clamps to
//! `created_at + MAX_RUNTIME_MS`, is refused past it, and every renewal stamps
//! `updated_at`. So a lease anything still holds is at most one `MAX_RUNTIME_MS`
//! stale, and the retention window is far longer — which is what lets the event
//! sweep key on age alone with no lease-liveness join, and what makes flipping
//! an aged `active` row safe.
//!
//! That premise is asserted at COMPILE time below. Grow the runtime ceiling past
//! the window and the build fails rather than shipping a sweep that deletes
//! running work.
//!
//! # The lifecycle tags are never eligible
//!
//! Only per-work events age out. An enrolment, a going-offline, a drain — those
//! are the record of what a host did across its whole life, and an operator
//! reading a six-month-old incident needs them to exist. Two lists rather than
//! one predicate, so adding an event type forces a decision about which side it
//! lands on.
//!
//! # A saturated cycle comes back in a minute, not an hour
//!
//! A cycle that filled every batch it was allowed means the backlog outran the
//! cycle's ceiling. Idling an hour after that would cap throughput at
//! `MAX_BATCHES × BATCH_LIMIT` rows per table per replica-hour, and a sustained
//! lease rate above that grows the backlog while every cycle reports success.
//! Only the idle GAP shrinks — the per-statement batch limit still bounds lock
//! time and write-ahead log, which is what makes the sweep safe to run at all.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use afd_core::clock;
use afd_core::timing::MAX_RUNTIME_MS;
use afd_db::Db;

use crate::error::{Result, query};

/// One retention statement, part-way through being bound.
///
/// Named because all three passes take and answer it, and the full spelling
/// would be the widest thing in this file.
type Statement = sqlx::query::Query<'static, sqlx::Postgres, sqlx::postgres::PgArguments>;
use crate::sql;
use crate::sweep::{Sweep, Swept};

/// Statement name, for the context a query failure carries.
const CONTEXT_LEASES: &str = "retention lease delete";

/// Statement name, for the context a query failure carries.
const CONTEXT_EVENTS: &str = "retention event delete";

/// Statement name, for the context a query failure carries.
const CONTEXT_ABANDONED: &str = "retention abandoned lease expiry";

/// How much history is kept past settlement.
///
/// A month of row-level forensics, which is what an incident review a few weeks
/// after the fact needs.
const RETENTION_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// The premise the whole age-keyed design rests on.
///
/// `retention_sweeper.zig` states it as a `comptime` block for the same reason:
/// if a lease could outlive the retention window, this sweep would delete rows
/// belonging to work still running, and it would do it silently.
const _: () = assert!(
    MAX_RUNTIME_MS < RETENTION_WINDOW_MS,
    "the retention window must exceed the maximum lease runtime, or an age-keyed sweep reaches live work"
);

/// One statement's row ceiling — bounds lock time and write-ahead log.
const BATCH_LIMIT: i64 = 1_000;

/// Batches per table per cycle.
///
/// Bounds a cycle's total work, so a backlog drains across cycles rather than
/// monopolising a connection for as long as it takes.
const MAX_BATCHES: usize = 8;

/// How long between cycles that drained everything they found.
const IDLE_INTERVAL: Duration = Duration::from_hours(1);

/// How long after a cycle that filled every batch.
const SATURATED_INTERVAL: Duration = Duration::from_mins(1);

/// The lease statuses whose rows are settled and may be deleted.
const TERMINAL_STATUSES: [&str; 2] = [sql::LEASE_STATUS_REPORTED, sql::LEASE_STATUS_EXPIRED];

/// The event types that belong to ONE lease, and age out with it.
///
/// The lifecycle tags are deliberately absent — see the module note.
const PER_WORK_EVENTS: [&str; 2] = [
    sql::event_type::LEASE_ACQUIRED,
    sql::event_type::LEASE_RELEASED,
];

/// The retention pass, over the api-role pool.
#[derive(Debug)]
pub struct Retention {
    /// Where the rows are.
    database: Db,
    /// Whether the last cycle filled every batch it was allowed.
    ///
    /// Read by [`Sweep::interval`] on the next tick, which is why it is atomic
    /// rather than a field: the trait hands out `&self`, and this is the one
    /// piece of state a pass leaves behind for the loop.
    saturated: AtomicBool,
}

impl Retention {
    /// A sweeper deleting through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self {
            database,
            saturated: AtomicBool::new(false),
        }
    }

    /// Runs one statement until it stops finding work, or hits the cycle's
    /// ceiling.
    ///
    /// Answers the rows it touched and whether it was still finding FULL
    /// batches when it stopped — which is what saturation means, and what the
    /// interval reads.
    ///
    /// The binder is a plain synchronous function rather than an async closure:
    /// what varies between the three passes is which parameters go on the
    /// statement, and nothing about that is asynchronous. Awaiting stays here,
    /// once, where the batching lives.
    async fn drain<B>(
        &self,
        context: &'static str,
        statement: &'static str,
        bind: B,
    ) -> Result<(u64, bool)>
    where
        B: Fn(Statement) -> Statement,
    {
        let mut total = 0;
        for _pass in 0..MAX_BATCHES {
            let mut connection = self.database.acquire().await?;
            let affected = bind(sqlx::query(statement))
                .execute(&mut *connection)
                .await
                .map(|done| done.rows_affected())
                .map_err(query(context))?;
            total += affected;
            // A short batch means the table is drained: there was nothing left
            // for the limit to cut off.
            if affected < u64::try_from(BATCH_LIMIT).unwrap_or(u64::MAX) {
                return Ok((total, false));
            }
        }
        Ok((total, true))
    }
}

impl Sweep for Retention {
    fn name(&self) -> &'static str {
        "retention"
    }

    /// Adaptive, and the only sweeper here that is.
    ///
    /// Read on every tick rather than once at start-up, which is what lets a
    /// backlog pull the next cycle in without the loop knowing anything about
    /// retention.
    fn interval(&self) -> Duration {
        if self.saturated.load(Ordering::Relaxed) {
            SATURATED_INTERVAL
        } else {
            IDLE_INTERVAL
        }
    }

    async fn sweep(&self) -> Result<Swept> {
        let now = clock::now().as_millis();
        let cutoff = now.saturating_sub(RETENTION_WINDOW_MS);

        // Abandoned leases are flipped FIRST, so the rows this cycle strands
        // become deletable by the pass below rather than waiting a whole
        // window longer.
        let (expired, expired_full) = self
            .drain(
                CONTEXT_ABANDONED,
                sql::sweep::EXPIRE_ABANDONED_ACTIVE_LEASES_BATCH,
                |statement| {
                    statement
                        .bind(vec![sql::LEASE_STATUS_ACTIVE])
                        .bind(cutoff)
                        .bind(sql::LEASE_STATUS_EXPIRED)
                        .bind(BATCH_LIMIT)
                        .bind(now)
                },
            )
            .await?;

        let (leases, leases_full) = self
            .drain(
                CONTEXT_LEASES,
                sql::sweep::DELETE_TERMINAL_LEASES_BATCH,
                |statement| {
                    statement
                        .bind(TERMINAL_STATUSES.to_vec())
                        .bind(cutoff)
                        .bind(BATCH_LIMIT)
                },
            )
            .await?;

        let (events, events_full) = self
            .drain(
                CONTEXT_EVENTS,
                sql::sweep::DELETE_AGED_RUNNER_EVENTS_BATCH,
                |statement| {
                    statement
                        .bind(PER_WORK_EVENTS.to_vec())
                        .bind(cutoff)
                        .bind(BATCH_LIMIT)
                },
            )
            .await?;

        self.saturated.store(
            expired_full || leases_full || events_full,
            Ordering::Relaxed,
        );
        let changed = expired.saturating_add(leases).saturating_add(events);
        Ok(Swept {
            // Every row this pass reached is a row it changed: the statements
            // select exactly what they then delete or flip, so there is no
            // "considered and left alone" for these three.
            scanned: changed,
            changed,
        })
    }
}

#[cfg(test)]
mod tests;
