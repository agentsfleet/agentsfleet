//! The replay half: re-appending admitted rows the queue never confirmed.
//!
//! # One transaction, committed whatever the queue did
//!
//! The batch is read `FOR UPDATE SKIP LOCKED`, so replicas take disjoint rows
//! and an admission still recording its own receipt waits behind this pass
//! rather than racing it. Every append that succeeds has its receipt written
//! inside the same transaction, and the transaction COMMITS even when a later
//! append fails: rolling it back would forget receipts for entries that are
//! already on the stream, and the next pass would append them a second time.
//! A row whose append failed keeps its NULL receipt and is retried; the
//! failure is logged as the pass's outcome, not raised past it.
//!
//! # A replayed entry can be a second physical entry
//!
//! An inserter that appended, died before recording the receipt, and is
//! replayed here leaves two entries carrying one event id. That is the
//! architecture page's stated shape: the lease path's `fleet.events`
//! conflict arm drops the second at delivery, and settlement keys on the
//! event id. Nothing here tries to be exactly-once on the stream, because
//! nothing can be.

use std::time::Duration;

use sqlx::Acquire as _;

use crate::error::{Result, query};
use crate::{Admissions, sql};

mod append;
mod confirm;
mod row;

pub use confirm::Confirmation;
use row::Pending;

/// Statement name, for the context a query failure carries.
const CONTEXT_REPLAY: &str = "replay an admission";

/// What one replay pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Replayed {
    /// Rows this pass found without a receipt and old enough to replay.
    pub scanned: u64,
    /// Rows it appended and recorded a receipt for.
    pub appended: u64,
}

impl Replayed {
    /// Whether every scanned row was appended.
    ///
    /// The pacing question a sweeper asks: a pass that appended everything it
    /// scanned may come straight back for more, and one that did not is one
    /// the queue refused, which coming back sooner would not fix.
    #[must_use]
    pub const fn is_clean(self) -> bool {
        self.scanned == self.appended
    }
}

impl Admissions {
    /// Re-appends every admitted row older than `min_age` that never got a
    /// receipt, up to `limit` rows.
    ///
    /// `min_age` is the window an in-flight admission is given to record its
    /// own receipt; a pass that ran with none would re-append entries their
    /// inserters were about to confirm. It is measured against the database's
    /// clock, the one each row's `created_at` was stamped by, so a replica
    /// whose own clock drifts cannot widen or close the window.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A queue that would not take
    /// an append is the pass's OUTCOME rather than its error — the rows keep
    /// their NULL receipt, the count says how far the pass got, and the next
    /// pass retries.
    pub async fn replay(&self, min_age: Duration, limit: i64) -> Result<Replayed> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_REPLAY))?;
        let rows = sqlx::query_as::<_, Pending>(sql::SELECT_UNRECEIPTED)
            .bind(whole_micros(min_age))
            .bind(limit)
            .fetch_all(&mut *transaction)
            .await
            .map_err(query(CONTEXT_REPLAY))?;

        let mut replayed = Replayed {
            scanned: u64::try_from(rows.len()).unwrap_or(u64::MAX),
            appended: 0,
        };
        let mut fleets: Vec<String> = Vec::new();
        for pending in rows {
            if !self.append_pending(&pending, &mut transaction).await? {
                break;
            }
            replayed.appended += 1;
            if !fleets.contains(&pending.fleet_id) {
                fleets.push(pending.fleet_id);
            }
        }
        transaction.commit().await.map_err(query(CONTEXT_REPLAY))?;

        for fleet in &fleets {
            self.mark_ready(fleet).await;
        }
        Ok(replayed)
    }
}

/// `span` floored to the microsecond, the finest an `interval` holds.
///
/// sqlx refuses to encode a `Duration` carrying nanoseconds an `interval`
/// cannot, so an age a caller computed from a nanosecond clock would fail the
/// whole pass; the floor keeps it an age, a microsecond younger at most.
fn whole_micros(span: Duration) -> Duration {
    Duration::from_micros(u64::try_from(span.as_micros()).unwrap_or(u64::MAX))
}
