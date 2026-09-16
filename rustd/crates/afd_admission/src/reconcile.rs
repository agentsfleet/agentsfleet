//! The reconcile half: accepted work whose queue entry the datastore lost.
//!
//! # The class the replay sweeper cannot see
//!
//! [`Admissions::replay`](crate::Admissions::replay) recovers rows with no
//! receipt. A row that GOT its receipt and was never delivered is invisible to
//! it, and that is the row a flush destroys: the producer was told yes, the
//! entry is gone, the consumer group went with it, and the readiness mark with
//! that. Nothing polls the fleet, so nothing notices. This pass is what
//! notices.
//!
//! It repairs by FORGETTING the receipt rather than by appending. A voided row
//! is back in the state the replay sweeper already scans, so there is one
//! append path in this crate and not two — the second would have to get
//! fencing, receipts, `replay_count` and the ready mark right all over again.
//!
//! # Why a missing entry proves destruction
//!
//! Retention is bounded below by unfinished work
//! (`afd_dragonfly::streams::retain`): the trim floor is the least of the
//! group's last delivered id, its oldest pending id, and the id 1,000 entries
//! from the tail, and a stream with no group at all is not trimmed. An
//! undelivered entry sits above the last delivered id, so no trim can reach
//! it. Nothing else in this daemon deletes an entry. A receipt the stream
//! cannot produce is therefore data loss, never housekeeping.
//!
//! # One question per fleet, and a walk only where it fails
//!
//! A fleet with undelivered work is ordinarily just a fleet whose runner has
//! not reached it. Asking the datastore about every such row every pass would
//! be round trips spent proving nothing, so the pass asks one question per
//! fleet — does the stream still hold this fleet's OLDEST undelivered receipt
//! — and walks row by row only where the answer is no. A rebuilt stream can
//! already hold new, live entries, which is why the walk still asks per row
//! instead of voiding the fleet wholesale.
//!
//! # A datastore that will not answer changes nothing
//!
//! Every probe failure leaves the row alone. Voiding on an unreadable stream
//! would re-append work the stream may still be holding, and the outage that
//! makes a probe fail is the one the ledger already survives by waiting.
//!
//! # Voiding spends the deployment's replay budget, on purpose
//!
//! A voided row re-enters `receipt IS NULL`, which is the count
//! [`crate::budget::REPLAY_BACKLOG_BUDGET`] caps and the admission `INSERT`
//! checks. A deployment recovering from a large loss therefore refuses new
//! producers with the retryable class until the sweeper drains what it voided.
//!
//! That is the budget doing its job rather than a side effect to engineer
//! around: the work is genuinely owed again, and admitting more on top of a
//! backlog nothing has drained is what the budget exists to stop. It is the
//! reason this pass takes a row cap per fleet instead of voiding everything it
//! finds — recovery arrives over several passes, and the backlog rises in
//! steps the sweeper can keep up with.

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_dragonfly::{EventId, FleetStreams};
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::{Admissions, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_RECONCILE: &str = "reconcile an admission's receipt";

/// A fleet whose stream could not produce its oldest undelivered receipt.
const EVENT_STREAM_LOST: &str = "admission_stream_data_lost";

/// A receipt was forgotten so the replay sweeper can re-append its row.
const EVENT_RECEIPT_VOIDED: &str = "admission_receipt_voided";

/// A datastore that would not answer a probe.
const EVENT_PROBE_FAILED: &str = "admission_reconcile_probe_failed";

/// What one reconcile pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reconciled {
    /// Fleets whose oldest undelivered receipt this pass asked about.
    pub probed: u64,
    /// Of those, fleets whose stream could not produce it.
    pub lost: u64,
    /// Rows whose receipt was forgotten, and which the replay sweeper now owes.
    pub voided: u64,
}

impl Reconciled {
    /// Whether this pass found nothing to repair.
    ///
    /// The pacing question a sweeper asks, and the answer on every pass of a
    /// healthy deployment: a pass that voided nothing can wait, and one that
    /// voided rows has handed the replay sweeper work worth coming back for.
    #[must_use]
    pub const fn is_quiet(self) -> bool {
        self.voided == 0
    }
}

/// One fleet's oldest admission that is receipted and not delivered.
struct Unfinished {
    fleet_id: String,
    receipt: EventId,
}

impl Admissions {
    /// Forgets the receipt of every admitted row whose queue entry the
    /// datastore no longer holds, so the replay sweeper re-appends it.
    ///
    /// `fleets` caps how many fleets one pass examines and `rows` how many of
    /// one fleet's undelivered admissions it repairs, so a deployment that lost
    /// everything is recovered over several passes instead of in one
    /// transaction holding every row.
    ///
    /// # Errors
    /// Reports a database that would not answer. A DATASTORE that would not
    /// answer is not an error: the probe is logged, the row keeps its receipt,
    /// and the next pass asks again — see the module docs.
    pub async fn reconcile(&self, now: UnixMillis, fleets: i64, rows: i64) -> Result<Reconciled> {
        let mut reconciled = Reconciled::default();
        for unfinished in self.unfinished_fleets(fleets).await? {
            reconciled.probed += 1;
            if self.stream_holds(&unfinished).await {
                continue;
            }
            reconciled.lost += 1;
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let fleet_id = unfinished.fleet_id.as_str();
            let receipt = unfinished.receipt.as_str();
            tracing::warn!(
                error_code = code,
                fleet_id,
                receipt,
                event = EVENT_STREAM_LOST,
                "the queue no longer holds this fleet's oldest undelivered entry, so its accepted work is being re-appended from the ledger"
            );
            reconciled.voided += self.void_lost_on(fleet_id, now, rows).await?;
        }
        Ok(reconciled)
    }

    /// One row per fleet that holds undelivered work: its oldest such row.
    async fn unfinished_fleets(&self, fleets: i64) -> Result<Vec<Unfinished>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_UNDELIVERED_FLEETS)
            .bind(fleets)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECONCILE))?;
        rows.iter()
            .map(|row| {
                Ok(Unfinished {
                    fleet_id: row.try_get(0).map_err(query(CONTEXT_RECONCILE))?,
                    receipt: EventId::of(
                        &row.try_get::<String, _>(1)
                            .map_err(query(CONTEXT_RECONCILE))?,
                    ),
                })
            })
            .collect()
    }

    /// Whether the fleet's stream still holds that entry.
    ///
    /// A probe that could not be made answers TRUE — "still there" — which is
    /// the answer that changes nothing. Logged rather than raised: one
    /// unreachable fleet must not end a pass that has others to examine.
    async fn stream_holds(&self, unfinished: &Unfinished) -> bool {
        let fleet_id = unfinished.fleet_id.as_str();
        match FleetStreams::new(self.queue.clone())
            .holds_entry(fleet_id, &unfinished.receipt)
            .await
        {
            Ok(held) => held,
            Err(unreachable_queue) => {
                let code = unreachable_queue.code().as_str();
                let reason = unreachable_queue.to_string();
                let receipt = unfinished.receipt.as_str();
                tracing::warn!(
                    error_code = code,
                    fleet_id,
                    receipt,
                    reason,
                    event = EVENT_PROBE_FAILED,
                    "the queue would not say whether it still holds this entry, so the admission keeps its receipt"
                );
                true
            }
        }
    }

    /// Voids every undelivered receipt on one fleet that the stream cannot
    /// produce, up to `rows`, and answers how many.
    ///
    /// No transaction, and the pool connection is never held across a probe:
    /// the candidates are read and the connection goes back, each probe runs
    /// with nothing held, and each void is its own short statement. A probe is
    /// a round trip to the OTHER datastore, and the rows a lock here would
    /// hold are the ones a live producer recording its own receipt waits
    /// behind — the reason [`sql::SELECT_UNDELIVERED_FLEETS`] gives for not
    /// locking, applied to the scan that probes per row.
    ///
    /// [`sql::VOID_LOST_RECEIPT`] pins the receipt it was told about, so a row
    /// the replay sweeper moved between the probe and the write matches
    /// nothing. That is also what a second replica walking this fleet hits:
    /// both probe, one writes, and the other counts the repair it did not make
    /// as the zero it was.
    async fn void_lost_on(&self, fleet_id: &str, now: UnixMillis, rows: i64) -> Result<u64> {
        let mut voided = 0;
        for (id, receipt) in self.undelivered_on(fleet_id, rows).await? {
            let still_held = Unfinished {
                fleet_id: fleet_id.to_owned(),
                receipt,
            };
            if self.stream_holds(&still_held).await {
                continue;
            }
            let receipt = still_held.receipt.as_str();
            let forgotten = self.void(&id, receipt, now).await?;
            voided += forgotten;
            if forgotten > 0 {
                tracing::info!(
                    fleet_id,
                    receipt,
                    event = EVENT_RECEIPT_VOIDED,
                    "this admission's entry is gone, so its receipt was forgotten and the replay sweeper owes it again"
                );
            }
        }
        Ok(voided)
    }

    /// One fleet's receipted-but-undelivered rows, read and released.
    ///
    /// Collected rather than streamed so the connection is back in the pool
    /// before the first probe, which is the whole point of the shape.
    async fn undelivered_on(&self, fleet_id: &str, rows: i64) -> Result<Vec<(String, EventId)>> {
        let mut connection = self.database.acquire().await?;
        let unfinished = sqlx::query(sql::SELECT_UNDELIVERED_ON_FLEET)
            .bind(fleet_id)
            .bind(rows)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECONCILE))?;
        unfinished
            .iter()
            .map(|row| {
                let id: String = row.try_get(0).map_err(query(CONTEXT_RECONCILE))?;
                let receipt: String = row.try_get(1).map_err(query(CONTEXT_RECONCILE))?;
                Ok((id, EventId::of(&receipt)))
            })
            .collect()
    }

    /// Forgets one row's receipt, answering whether this statement did it.
    ///
    /// Zero is not a failure: it means the row no longer carries the receipt
    /// this pass probed, so somebody else already repaired it or a delivery
    /// landed first.
    async fn void(&self, id: &str, receipt: &str, now: UnixMillis) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let voided = sqlx::query(sql::VOID_LOST_RECEIPT)
            .bind(id)
            .bind(now.as_millis())
            .bind(receipt)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECONCILE))?;
        Ok(voided.rows_affected())
    }
}
