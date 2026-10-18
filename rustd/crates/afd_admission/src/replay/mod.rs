//! The replay half: re-appending admitted rows the queue never confirmed.
//!
//! # Claim, append, record: no transaction spans the queue
//!
//! A pass claims its batch in one statement that commits on its own
//! ([`sql::CLAIM_UNRECEIPTED`]): replicas take disjoint rows, and the claim is
//! a stamp rather than a row lock, so a slow or partitioned queue holds no
//! pooled connection and no lock while the pass appends. The appends run with
//! nothing open in Postgres, stop at the first refusal, and the receipts they
//! answered are recorded in one statement that also clears their claims;
//! rows the queue never reached are released for the next pass. A pass that
//! dies anywhere in between leaves claims that lapse after [`CLAIM_SPAN`], and
//! the rows are replayed then.
//!
//! # A replayed entry can be a second physical entry
//!
//! An inserter that appended, died before recording the receipt, and is
//! replayed here leaves two entries carrying one event id; so does a pass
//! that appended and then could not record, or one that overran its claim.
//! That is the architecture page's stated shape: the lease path's
//! `fleet.events` conflict arm drops the second at delivery, and settlement
//! keys on the event id. Nothing here tries to be exactly-once on the stream,
//! because nothing can be.

use std::time::Duration;

use crate::error::{Result, query};
use crate::{Admissions, sql};

mod append;
mod confirm;
mod pass;
mod row;

pub use confirm::Confirmation;
use row::Pending;

/// Statement name, for the context a query failure carries.
const CONTEXT_REPLAY: &str = "replay an admission";

/// How long a claim keeps a row out of every other pass and confirmation.
///
/// Longer than a full batch of appends at the queue's request timeout, so a
/// live pass does not lose its rows to a sibling; a pass that overruns it
/// costs a second physical entry, which delivery drops by the event id. It is
/// also how long a row a dead pass claimed waits before it is replayed.
const CLAIM_SPAN: Duration = Duration::from_mins(5);

/// What one replay pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Replayed {
    /// Rows this pass found without a receipt and old enough to replay.
    pub scanned: u64,
    /// Rows the queue took. A row whose inserter recorded its own receipt
    /// while the pass appended counts here too: its entry is on the stream.
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
    /// their NULL receipt and are released, the count says how far the pass
    /// got, and the next pass retries.
    pub async fn replay(&self, min_age: Duration, limit: i64) -> Result<Replayed> {
        let rows: Vec<Pending> = sqlx::query_as(sql::CLAIM_UNRECEIPTED)
            .bind(whole_micros(min_age))
            .bind(limit)
            .bind(CLAIM_SPAN)
            .fetch_all(&mut *self.database.acquire().await?)
            .await
            .map_err(query(CONTEXT_REPLAY))?;

        let drained = pass::drain(&rows, |pending| self.append_pending(pending)).await;
        self.record_replays(&drained).await?;

        let mut fleets: Vec<&str> = Vec::new();
        for (pending, _) in &drained.appended {
            if !fleets.contains(&pending.fleet_id.as_str()) {
                fleets.push(&pending.fleet_id);
            }
        }
        for fleet in fleets {
            self.mark_ready(fleet).await;
        }
        Ok(Replayed {
            scanned: u64::try_from(rows.len()).unwrap_or(u64::MAX),
            appended: u64::try_from(drained.appended.len()).unwrap_or(u64::MAX),
        })
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
