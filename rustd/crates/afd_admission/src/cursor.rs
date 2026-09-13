//! What the ledger can say about a stream from Postgres alone: where a lost
//! consumer group should resume, and how far behind the queue is.
//!
//! # The restore cursor is a join, and the join is the point
//!
//! A receipt on an admission says the entry REACHED the stream. Whether it was
//! ever handed to a runner is recorded in `core.fleet_events`, whose row the
//! lease path writes on delivery. The newest receipt among admissions that
//! have such a row is therefore the newest entry that ran, and a group
//! recreated there offers exactly what has not: everything after it. That
//! table belongs to `afd_events`, and reading it from here is the same
//! trespass the lease path's reclaim already makes for the same reason — the
//! question cannot be answered from one table.

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_datastore::{EventId, GroupCursor};
use sqlx::Row as _;

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
    /// hundred thousand.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn backlog(&self, now: UnixMillis) -> Result<LedgerBacklog> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_BACKLOG)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_BACKLOG))?;
        let rows: i64 = row.try_get(0).map_err(query(CONTEXT_BACKLOG))?;
        let oldest: Option<i64> = row.try_get(1).map_err(query(CONTEXT_BACKLOG))?;
        Ok(LedgerBacklog {
            rows: u64::try_from(rows).unwrap_or(0),
            oldest_age: oldest.map(|created_at| {
                Duration::from_millis(
                    u64::try_from(now.as_millis().saturating_sub(created_at)).unwrap_or(0),
                )
            }),
        })
    }
}
