//! What the ledger can say about a stream from Postgres alone: where a lost
//! consumer group should resume, and how far behind the queue is.
//!
//! # The restore cursor is a join, and the join is the point
//!
//! A receipt on an admission says the entry REACHED the stream. Whether it was
//! ever handed to a runner is recorded in `fleet.events`, whose row the
//! lease path writes on delivery. The newest receipt among admissions that
//! have such a row is therefore the newest entry that ran, and a group
//! recreated there offers exactly what has not: everything after it. That
//! table belongs to `afd_events`, and reading it from here is the same
//! trespass the lease path's reclaim already makes for the same reason — the
//! question cannot be answered from one table.

use std::time::Duration;

use afd_dragonfly::{EventId, GroupCursor};
use sqlx::postgres::types::PgInterval;

use crate::error::{Result, query};
use crate::{Admissions, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_CURSOR: &str = "read a fleet's delivered cursor";

/// Statement name, for the context a query failure carries.
const CONTEXT_BACKLOG: &str = "read the replay backlog";

/// How far behind the queue is, as the ledger sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LedgerBacklog {
    /// Admitted rows the queue has not confirmed.
    pub rows: u64,
    /// How long the oldest of them has waited; `None` when there are none.
    pub oldest_age: Option<Duration>,
}

impl Admissions {
    /// Where a recreated consumer group for `fleet` should start delivering.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn delivered_cursor(&self, fleet: &str) -> Result<GroupCursor> {
        let mut connection = self.database.acquire().await?;
        let receipt: Option<String> = sqlx::query_scalar(sql::SELECT_DELIVERED_CURSOR)
            .bind(fleet)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_CURSOR))?;
        Ok(receipt.map_or(GroupCursor::Beginning, |receipt| {
            GroupCursor::After(EventId::of(&receipt))
        }))
    }

    /// How many rows await a receipt, and how long the oldest has.
    ///
    /// Read whole rather than from a replay batch: a batch is capped, and a
    /// capped count reported as the backlog would say "thirty-two" of a
    /// hundred thousand. The age is the database's own subtraction, against
    /// the clock that stamped the rows.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn backlog(&self) -> Result<LedgerBacklog> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, BacklogRow>(sql::SELECT_BACKLOG)
            .fetch_one(&mut *connection)
            .await
            .map(BacklogRow::into_backlog)
            .map_err(query(CONTEXT_BACKLOG))
    }
}

/// The one row [`sql::SELECT_BACKLOG`] answers.
#[derive(Debug, sqlx::FromRow)]
struct BacklogRow {
    waiting: i64,
    oldest_age: Option<PgInterval>,
}

impl BacklogRow {
    fn into_backlog(self) -> LedgerBacklog {
        LedgerBacklog {
            rows: u64::try_from(self.waiting).unwrap_or(0),
            oldest_age: self.oldest_age.map(age),
        }
    }
}

/// Microseconds in one day, the unit `interval` keeps apart from its time.
const MICROS_PER_DAY: i64 = 86_400_000_000;

/// An age as Postgres subtracts two instants: days and microseconds, never
/// months.
///
/// A negative age — a row stamped by a transaction the reading statement began
/// before — reads as none, since the row has not waited at all.
fn age(interval: PgInterval) -> Duration {
    let micros = i64::from(interval.days)
        .saturating_mul(MICROS_PER_DAY)
        .saturating_add(interval.microseconds);
    Duration::from_micros(u64::try_from(micros).unwrap_or(0))
}
