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
//! # One question per fleet, a walk where it fails, and a cursor under both
//!
//! A fleet with undelivered work is ordinarily just a fleet whose runner has
//! not reached it. Asking the datastore about every such row every pass would
//! be round trips spent proving nothing, so the pass asks one question per
//! fleet — does the stream still hold this fleet's OLDEST undelivered receipt
//! — and walks row by row only where the answer is no. A rebuilt stream can
//! already hold new, live entries, which is why the walk still asks per row
//! instead of voiding the fleet wholesale.
//!
//! Both of those are bounded, and both bounds used to be permanent. More
//! unfinished fleets than one pass examines meant the ones sorting last were
//! never examined; more lost rows on a fleet than one walk repairs meant the
//! rows past the batch hid behind the ones the repair had just made healthy.
//! [`Progress`] carries the resume point for each, and its module note has both
//! failures worked through. The pass is in two parts because of it: fleets
//! already known to be losing rows are continued from where their walk stopped,
//! and only then does the head-probe sweep rotate on.
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

use crate::Admissions;
use crate::error::Result;

mod progress;
mod scan;

use self::progress::RowKey;
pub use self::progress::{DEFAULT_REPAIR_CAPACITY, Progress};

/// Statement name, for the context a query failure carries.
pub(crate) const CONTEXT_RECONCILE: &str = "reconcile an admission's receipt";

/// A fleet whose stream could not produce its oldest undelivered receipt.
const EVENT_STREAM_LOST: &str = "admission_stream_data_lost";

/// A receipt was forgotten so the replay sweeper can re-append its row.
const EVENT_RECEIPT_VOIDED: &str = "admission_receipt_voided";

/// A datastore that would not answer a probe.
const EVENT_PROBE_FAILED: &str = "admission_reconcile_probe_failed";

/// A fleet with lost rows left that the resume set had no room to remember.
const EVENT_REPAIR_DECLINED: &str = "admission_reconcile_repair_declined";

/// What one reconcile pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reconciled {
    /// Fleets this pass asked the datastore about, head probes and continued
    /// repairs together.
    pub probed: u64,
    /// Of those, fleets whose stream could not produce it.
    pub lost: u64,
    /// Rows whose receipt was forgotten, and which the replay sweeper now owes.
    pub voided: u64,
    /// Whether a fleet is known to hold lost rows this pass did not reach.
    pub resuming: bool,
    /// Repairs this pass had to decline for want of room to remember them.
    pub declined: u64,
}

impl Reconciled {
    /// Whether this pass found nothing to repair.
    ///
    /// The pacing question a sweeper asks, and the answer on every pass of a
    /// healthy deployment: a pass that voided nothing can wait, and one that
    /// voided rows has handed the replay sweeper work worth coming back for.
    ///
    /// A pass that voided nothing and still left a fleet mid-repair is NOT
    /// quiet. It found real loss and ran out of budget, so waiting the idle
    /// interval would pace recovery at five minutes a batch.
    #[must_use]
    pub const fn is_quiet(self) -> bool {
        self.voided == 0 && !self.resuming
    }
}

/// One fleet's oldest admission that is receipted and not delivered.
pub(crate) struct Unfinished {
    pub(crate) fleet_id: String,
    pub(crate) receipt: EventId,
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
    pub async fn reconcile(
        &self,
        now: UnixMillis,
        fleets: i64,
        rows: i64,
        progress: &mut Progress,
    ) -> Result<Reconciled> {
        let mut reconciled = Reconciled::default();
        self.continue_repairs(now, fleets, rows, progress, &mut reconciled)
            .await?;
        self.sweep_heads(now, fleets, rows, progress, &mut reconciled)
            .await?;
        reconciled.resuming = progress.is_resuming();
        Ok(reconciled)
    }

    /// Walks on from where each mid-repair fleet's last batch stopped.
    ///
    /// Before the head probes and not after, because these fleets are the ones
    /// the head probe would get WRONG: their oldest undelivered receipt is a
    /// row the last pass voided and the replay sweeper has since re-appended,
    /// so the stream holds it and the shortcut reports health over rows that
    /// are still lost. Asking the stream about the fleet again would spend a
    /// round trip to be told the wrong thing.
    async fn continue_repairs(
        &self,
        now: UnixMillis,
        fleets: i64,
        rows: i64,
        progress: &mut Progress,
        reconciled: &mut Reconciled,
    ) -> Result<()> {
        let mut queued = progress.resume_repairs(fleets).into_iter();
        while let Some(repair) = queued.next() {
            reconciled.probed += 1;
            let walked = match self
                .void_lost_on(&repair.fleet_id, now, rows, repair.after)
                .await
            {
                Ok(walked) => walked,
                Err(unanswered) => {
                    // These were DRAINED out of the set to be walked, and this
                    // pass is not going to walk them. Putting them back is what
                    // keeps a database blip from costing every queued fleet its
                    // resume point — see `Progress::refile`.
                    for unwalked in std::iter::once(repair).chain(queued) {
                        Self::note_declined(
                            &unwalked.fleet_id,
                            progress.refile(unwalked.clone()),
                            reconciled,
                        );
                    }
                    return Err(unanswered);
                }
            };
            reconciled.voided += walked.voided;
            Self::remember(&repair.fleet_id, &walked, progress, reconciled);
        }
        Ok(())
    }

    /// Asks each fleet in the rotation's next slice about its oldest receipt.
    async fn sweep_heads(
        &self,
        now: UnixMillis,
        fleets: i64,
        rows: i64,
        progress: &mut Progress,
        reconciled: &mut Reconciled,
    ) -> Result<()> {
        let heads = self
            .unfinished_fleets(fleets, progress.resume_from())
            .await?;
        let budget_filled = i64::try_from(heads.len()).is_ok_and(|read| read >= fleets);
        let last_seen = heads.last().map(|head| head.fleet_id.clone());
        for unfinished in heads {
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
            let walked = self
                .void_lost_on(fleet_id, now, rows, RowKey::FIRST)
                .await?;
            reconciled.voided += walked.voided;
            Self::remember(fleet_id, &walked, progress, reconciled);
        }
        progress.swept(last_seen, budget_filled);
        Ok(())
    }

    /// Files a walk's resume point, or records that there was no room for it.
    ///
    /// A declined repair is the one place this design trades coverage for its
    /// memory bound, so it is logged rather than dropped silently. What the
    /// operator is being told is not "this is slower": the fleet's remaining
    /// lost rows are invisible to the head probe until the rows this pass
    /// restored have been delivered, so on a fleet nothing is consuming they
    /// wait indefinitely. [`Progress`] carries the full statement.
    fn remember(
        fleet_id: &str,
        walked: &Walked,
        progress: &mut Progress,
        reconciled: &mut Reconciled,
    ) {
        Self::note_declined(
            fleet_id,
            progress.walked(fleet_id, walked.stopped_at),
            reconciled,
        );
    }

    /// Counts and logs a repair the resume set had no room for.
    fn note_declined(fleet_id: &str, filed: bool, reconciled: &mut Reconciled) {
        if filed {
            return;
        }
        reconciled.declined += 1;
        tracing::info!(
            fleet_id,
            event = EVENT_REPAIR_DECLINED,
            "this fleet has lost rows past the batch and the resume set is full, so its repair waits for a later pass"
        );
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
    async fn void_lost_on(
        &self,
        fleet_id: &str,
        now: UnixMillis,
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
            let forgotten = self.void(&candidate.id, receipt, now).await?;
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
struct Walked {
    /// Receipts forgotten by this walk.
    voided: u64,
    /// The row a full batch stopped at, or `None` when it reached the end.
    stopped_at: Option<RowKey>,
}
