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
//! replayed here leaves two entries carrying one logical id. That is the
//! architecture page's stated shape: the lease path's `core.fleet_events`
//! conflict arm drops the second at delivery, and settlement keys on the
//! logical id. Nothing here tries to be exactly-once on the stream, because
//! nothing can be.

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_datastore::FleetStreams;
use afd_observability::metrics::label::fleet::ReplayOutcome;
use afd_observability::producers::fleet::admission as metrics;
use afd_wire::event::Entry;
use sqlx::{Acquire as _, Postgres, Row as _, Transaction};

use crate::error::{Result, query};
use crate::{Admissions, logical_id, sql};

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

/// One admitted row awaiting its receipt.
struct Pending {
    id: String,
    fleet_id: String,
    workspace_id: String,
    actor: String,
    event_type: String,
    request_json: String,
    event_created_at: i64,
    logical_id: String,
}

impl Pending {
    /// Reads one [`sql::SELECT_UNRECEIPTED`] row.
    fn read(row: &sqlx::postgres::PgRow) -> Result<Self> {
        let text =
            |index: usize| -> Result<String> { row.try_get(index).map_err(query(CONTEXT_REPLAY)) };
        let number =
            |index: usize| -> Result<i64> { row.try_get(index).map_err(query(CONTEXT_REPLAY)) };
        Ok(Self {
            id: text(0)?,
            fleet_id: text(1)?,
            workspace_id: text(2)?,
            actor: text(3)?,
            event_type: text(4)?,
            request_json: text(5)?,
            event_created_at: number(6)?,
            logical_id: logical_id(number(7)?, number(8)?),
        })
    }
}

impl Admissions {
    /// Re-appends every admitted row older than `min_age` that never got a
    /// receipt, up to `limit` rows.
    ///
    /// `min_age` is the window an in-flight admission is given to record its
    /// own receipt; a pass that ran with none would re-append entries their
    /// inserters were about to confirm. `now` is the caller's so a sweeper's
    /// one instant stamps every receipt it records.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A queue that would not take
    /// an append is the pass's OUTCOME rather than its error — the rows keep
    /// their NULL receipt, the count says how far the pass got, and the next
    /// pass retries.
    pub async fn replay(&self, now: UnixMillis, min_age: Duration, limit: i64) -> Result<Replayed> {
        let cutoff = now
            .as_millis()
            .saturating_sub(i64::try_from(min_age.as_millis()).unwrap_or(i64::MAX));
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_REPLAY))?;
        let rows = sqlx::query(sql::SELECT_UNRECEIPTED)
            .bind(cutoff)
            .bind(limit)
            .fetch_all(&mut *transaction)
            .await
            .map_err(query(CONTEXT_REPLAY))?;

        let mut replayed = Replayed {
            scanned: u64::try_from(rows.len()).unwrap_or(u64::MAX),
            appended: 0,
        };
        let mut fleets: Vec<String> = Vec::new();
        for row in &rows {
            let pending = Pending::read(row)?;
            if !self.append_pending(&pending, now, &mut transaction).await? {
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

    /// Appends one pending row and records its receipt inside `transaction`.
    ///
    /// Answers whether the append landed. A queue refusal is logged and
    /// answered `false`; the caller stops the pass there, because a queue
    /// that refused one append will refuse the next.
    async fn append_pending(
        &self,
        pending: &Pending,
        now: UnixMillis,
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<bool> {
        let created_at = pending.event_created_at.to_string();
        let entry = Entry {
            actor: &pending.actor,
            // The stored spelling, passed through. It was written by
            // `EventType::as_str` at admit time, and parsing it back only to
            // spell it again would let a row a NEWER daemon admitted be
            // replayed under a type this build happens to know.
            event_type: &pending.event_type,
            workspace_id: &pending.workspace_id,
            request_json: &pending.request_json,
            created_at: &created_at,
        };
        let event_id = pending.logical_id.as_str();
        let fleet_id = pending.fleet_id.as_str();
        let receipt = match FleetStreams::new(self.queue.clone())
            .append(fleet_id, &entry.queued_pairs(event_id))
            .await
        {
            Ok(receipt) => receipt,
            Err(unreachable_queue) => {
                let (outcome, event) = if unreachable_queue.is_full() {
                    (ReplayOutcome::Full, "admission_replay_queue_full")
                } else {
                    (ReplayOutcome::Failed, "admission_replay_failed")
                };
                metrics::replayed(outcome);
                let code = unreachable_queue.code().as_str();
                let reason = unreachable_queue.to_string();
                tracing::warn!(error_code = code, fleet_id, event_id, reason, event,);
                return Ok(false);
            }
        };

        // The count comes back so a row that keeps returning is visible as
        // something other than throughput: one re-append is the crash window
        // this sweeper exists for, and a fifth is a row that is being appended
        // and never receipted.
        let replays: i64 = sqlx::query_scalar(sql::RECORD_REPLAY_RECEIPT)
            .bind(pending.id.as_str())
            .bind(receipt.as_str())
            .bind(now.as_millis())
            .fetch_one(&mut **transaction)
            .await
            .map_err(query(CONTEXT_REPLAY))?;
        metrics::replayed(ReplayOutcome::Appended);
        let receipt_field = receipt.as_str();
        tracing::info!(
            fleet_id,
            event_id,
            receipt = receipt_field,
            replays,
            event = "admission_replayed_to_queue",
        );
        Ok(true)
    }
}
