//! Putting owed answers on the queue, and putting them back when it loses them.
//!
//! # What this closes
//!
//! The report path commits an obligation and then appends it, and between those
//! two things a process can die. This pass is what makes that survivable: a row
//! the queue never confirmed is re-appended here, so the answer reaches a
//! destination even though the process that produced it did not live long
//! enough to queue it. It is the same argument `afd_runner`'s admission replay
//! makes inbound, and the reason an obligation could be a row at all.
//!
//! # Two scans, two different losses
//!
//! **Unreceipted** is the crash above: committed, never queued.
//!
//! **Undelivered** is the queue itself failing — a consumer group deleted out
//! of band, a failover onto an empty replica, a stream lost with the datastore's
//! contents, or a worker replaced under a different hostname whose pending
//! entries nobody inherits. The queue cannot tell those apart and recovers from
//! none of them, because in every one the entry is gone while the answer is
//! still owed. PostgreSQL still holds the obligation, so this pass re-appends
//! it.
//!
//! That second scan is what makes losing Dragonfly cost latency rather than
//! answers, which is the property `datastore_scaling.md` states and Dimension
//! 7.8 has to prove.
//!
//! # Why re-appending is allowed to duplicate
//!
//! It is not free: a destination may receive the same answer twice. The path is
//! at-least-once and always was — a vendor call whose response is lost has
//! happened whether or not we heard so. The direction of the error is the
//! choice being made, and it is made deliberately: a duplicate message in a
//! thread is visible and recoverable by a person, while an answer silently
//! never sent is neither.
//!
//! `delivered_at` is what keeps the duplicate rare. It is stamped by the POSTER
//! on a verdict of delivered, not by the acknowledgement — which fires for an
//! exhausted job too — so a row leaves the undelivered set only when somebody
//! actually received it.

use std::time::Duration;

use afd_core::clock;
use afd_db::Db;
use afd_dragonfly::{OutboundJob, OutboundQueue};
use tokio_util::sync::CancellationToken;

use crate::obligation::{self, AbandonReason, Owed};

/// How long an obligation is left for its own committer before this pass takes
/// it.
///
/// A report between its commit and its append runs one statement and one
/// append. A floor near that duration would make this pass race the live report
/// and put a second entry on the queue every time it won.
pub const MIN_AGE: Duration = Duration::from_secs(30);

/// How long a queued answer is left before it is treated as lost.
///
/// Longer than [`MIN_AGE`] because a slow destination is not a lost entry: a
/// lane retrying against a rate-limited vendor is working, and re-appending
/// underneath it would deliver twice for no reason. This is the age at which
/// "still in a lane" stops being the likelier explanation than "the entry is
/// gone".
pub const LOST_AFTER: Duration = Duration::from_secs(300);

/// How many delivery cycles an answer is allowed before it is abandoned.
///
/// A cycle ends retryable only after its own backoff has run out, and a lost
/// answer is re-offered once per [`LOST_AFTER`], so this is roughly an hour of
/// a destination refusing before the answer stops costing the queue anything.
pub const MAX_DELIVERY_CYCLES: i64 = 12;

/// How many rows one pass takes from each scan.
pub const BATCH_LIMIT: i64 = 32;

/// How long between passes.
pub const INTERVAL: Duration = Duration::from_secs(30);

// Decidable at compile time, so checked there — a runtime test could only
// re-assert a constant a reader can see.
const _: () = {
    assert!(
        MIN_AGE.as_secs() >= 10,
        "a floor this low races the report it is meant to wait for"
    );
    assert!(
        LOST_AFTER.as_secs() > MIN_AGE.as_secs(),
        "a queued answer must be given longer than an unqueued one"
    );
    assert!(BATCH_LIMIT > 0, "a pass that takes no rows never drains");
    assert!(
        MAX_DELIVERY_CYCLES >= 2,
        "one cycle is no retry budget: a single outage would abandon the answer"
    );
    assert!(
        BATCH_LIMIT <= 128,
        "a batch this large holds the queue for a live report"
    );
    assert!(
        !INTERVAL.is_zero(),
        "an idle producer must wait between passes"
    );
};

/// The owed-answer producer.
#[derive(Debug)]
pub struct Producer {
    queue: OutboundQueue,
    database: Db,
}

impl Producer {
    /// A producer appending through `queue` and reading `database`.
    #[must_use]
    pub const fn new(queue: OutboundQueue, database: Db) -> Self {
        Self { queue, database }
    }

    /// Runs until the supervisor cancels `token`.
    ///
    /// A failed pass is logged and waited out rather than propagated, for the
    /// reason the worker's loop does the same: one unavailable store must not
    /// take the delivery path down for the life of the process.
    pub async fn run(self, token: CancellationToken) {
        tracing::debug!(event = "outbound_producer_started");
        loop {
            if token.is_cancelled() {
                break;
            }
            let appended = self.pass().await;
            if appended > 0 {
                tracing::info!(appended, event = "outbound_obligations_requeued");
            }
            tokio::select! {
                biased;
                () = token.cancelled() => break,
                () = tokio::time::sleep(INTERVAL) => {}
            }
        }
        tracing::debug!(event = "outbound_producer_shutdown");
    }

    /// One pass over both scans. Answers how many entries it appended.
    async fn pass(&self) -> u64 {
        let now = clock::now();
        let uncommitted_before = now
            .as_millis()
            .saturating_sub(i64::try_from(MIN_AGE.as_millis()).unwrap_or(i64::MAX));
        let lost_before = now
            .as_millis()
            .saturating_sub(i64::try_from(LOST_AFTER.as_millis()).unwrap_or(i64::MAX));

        let mut appended = 0;
        appended += self
            .requeue(
                obligation::unreceipted(
                    &self.database,
                    clock::UnixMillis::from_millis(uncommitted_before),
                    BATCH_LIMIT,
                )
                .await,
                "unreceipted",
            )
            .await;
        appended += self
            .requeue(
                obligation::undelivered(
                    &self.database,
                    clock::UnixMillis::from_millis(lost_before),
                    BATCH_LIMIT,
                )
                .await,
                "undelivered",
            )
            .await;
        appended
    }

    /// Append every row of one scan, recording each entry as its receipt.
    async fn requeue(&self, scanned: crate::error::Result<Vec<Owed>>, scan: &'static str) -> u64 {
        let rows = match scanned {
            Ok(rows) => rows,
            Err(failure) => {
                crate::worker::report("outbound_obligation_scan_failed", &failure);
                return 0;
            }
        };

        let mut appended = 0;
        for owed in rows {
            // The scans return only rows that name a destination, so a row that
            // will not address names a connector nobody answers to. No entry
            // could deliver it, and skipping it would leave it in every scan.
            let Some(delivery) = owed.addressed() else {
                crate::abandon::row(&self.database, &owed, AbandonReason::Unaddressable).await;
                continue;
            };
            let entry = match self.queue.enqueue(OutboundJob::from(delivery)).await {
                Ok(entry) => entry,
                Err(failure) => {
                    // The queue is refusing. Stop the pass rather than walk the
                    // rest of the batch into the same refusal — the next pass
                    // finds the same rows, and the row is still owed either way.
                    crate::worker::report("outbound_obligation_append_failed", &failure.into());
                    break;
                }
            };
            if let Err(failure) = obligation::record_reappended(
                &self.database,
                &owed.id,
                entry.as_str(),
                clock::now(),
            )
            .await
            {
                // The entry IS on the queue and will be delivered. What failed
                // is the record of it, so the next pass appends a second entry
                // and the destination may see the answer twice — the direction
                // this path already accepts.
                crate::worker::report("outbound_obligation_receipt_failed", &failure);
            }
            appended += 1;
        }
        if appended > 0 {
            tracing::debug!(scan, appended, event = "outbound_obligation_scan_requeued");
        }
        appended
    }
}
