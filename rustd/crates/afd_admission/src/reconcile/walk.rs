//! One fleet's walk: probe each undelivered receipt, and forget the ones the
//! stream can no longer produce.
//!
//! Split from the pass that drives it for length. The pass decides WHICH
//! fleets to walk and where each walk resumes; this is the walk itself, and
//! the probe it asks per row.

use afd_core::error_code::{Coded as _, Logged};
use afd_dragonfly::FleetStreams;

use super::progress::RowKey;
use super::{EVENT_PROBE_FAILED, EVENT_RECEIPT_VOIDED, Unfinished};
use crate::Admissions;
use crate::error::Result;

impl Admissions {
    /// Whether the fleet's stream still holds that entry.
    ///
    /// A probe that could not be made answers TRUE — "still there" — which is
    /// the answer that changes nothing. Logged rather than raised: one
    /// unreachable fleet must not end a pass that has others to examine.
    pub(super) async fn stream_holds(&self, unfinished: &Unfinished) -> bool {
        let fleet_id = unfinished.fleet_id.as_str();
        match FleetStreams::new(self.queue.clone())
            .holds_entry(fleet_id, &unfinished.receipt)
            .await
        {
            Ok(held) => held,
            Err(unreachable_queue) => {
                let Logged { error_code, reason } = unreachable_queue.logged();
                let receipt = unfinished.receipt.as_str();
                tracing::warn!(
                    error_code,
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
    /// produce, starting after `after`, up to `rows`.
    ///
    /// No transaction, and the pool connection is never held across a probe:
    /// the candidates are read and the connection goes back, each probe runs
    /// with nothing held, and each void is its own short statement. A probe is
    /// a round trip to the OTHER datastore, and the rows a lock here would hold
    /// are the ones a live producer recording its own receipt waits behind —
    /// the reason [`sql::SELECT_UNDELIVERED_FLEETS`] gives for not locking,
    /// applied to the scan that probes per row.
    ///
    /// [`sql::VOID_LOST_RECEIPT`] pins the receipt it was told about, so a row
    /// the replay sweeper moved between the probe and the write matches
    /// nothing. That is also what a second replica walking this fleet hits:
    /// both probe, one writes, and the other counts the repair it did not make
    /// as the zero it was.
    ///
    /// Answers where the next walk resumes. A batch that filled its limit says
    /// the last row it examined — LIVE rows included, because the point of the
    /// cursor is to move past everything this walk has already asked about.
    /// A short batch says nothing, which retires the fleet back to the head
    /// probe.
    pub(super) async fn void_lost_on(
        &self,
        fleet_id: &str,
        rows: i64,
        after: RowKey,
    ) -> Result<Walked> {
        let candidates = self.undelivered_on(fleet_id, rows, after).await?;
        let batch_filled = i64::try_from(candidates.len()).is_ok_and(|read| read >= rows);
        let stopped_at = batch_filled
            .then(|| candidates.last().map(|last| last.key))
            .flatten();
        let mut voided = 0;
        for candidate in candidates {
            let still_held = Unfinished {
                fleet_id: fleet_id.to_owned(),
                receipt: candidate.receipt,
            };
            if self.stream_holds(&still_held).await {
                continue;
            }
            let receipt = still_held.receipt.as_str();
            let forgotten = self.void(&candidate.id, receipt).await?;
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
        Ok(Walked { voided, stopped_at })
    }
}

/// What one fleet's walk did, and where the next one carries on.
pub(super) struct Walked {
    /// Receipts forgotten by this walk.
    pub(super) voided: u64,
    /// The row a full batch stopped at, or `None` when it reached the end.
    pub(super) stopped_at: Option<RowKey>,
}
