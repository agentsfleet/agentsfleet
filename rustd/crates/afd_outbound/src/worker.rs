//! The supervised task: read a job, deliver it, acknowledge it, repeat.
//!
//! # The loop, and the order it asks in
//!
//! Pending first, always. `XREADGROUP >` only ever hands out entries nobody has
//! seen, so an entry this consumer was handed and never acknowledged is
//! re-offered by nothing — it sits in the pending list until somebody asks for
//! it by name. A worker that only ever read `>` would therefore lose exactly
//! the jobs a restart was supposed to rescue, and lose them INVISIBLY: the
//! entry is neither delivered nor gone.
//!
//! Then a blocking read, raced against the supervisor's token. `BLOCK` is what
//! makes an answer leave the instant it is queued instead of up to a
//! quarter-second later, and it is why this worker owns an
//! [`afd_redis::Dedicated`] connection: parking the shared one would park every
//! other caller in the process behind it.
//!
//! # Cancellation stops the READ, never a delivery
//!
//! A job in hand is finished. Abandoning one mid-post would leave a vendor call
//! in flight with nothing to read its answer, and the durable stream would then
//! redeliver a job that may well have landed. So the token is selected over at
//! exactly one point — the blocking read — and inside a delivery it does one
//! narrower thing: it stops the RETRY loop from starting another attempt. The
//! attempt already running finishes, and a job whose last attempt failed is
//! simply not acknowledged, which is the same thing as re-queuing it.
//!
//! That bounds a shutdown at one vendor deadline rather than at the whole retry
//! budget, which is what keeps the join inside
//! [`agentsfleetd::supervisor::JOIN_TIMEOUT`] with room to spare.
//!
//! # Dropping the read future does not cancel the read
//!
//! The load-bearing fact behind Dimension 5.2. `tokio::select!` drops the
//! losing branch, but the `XREADGROUP` has already been WRITTEN to the socket:
//! Redis may assign an entry to this consumer after this process has stopped
//! caring. The entry is not lost — it is pending, under a consumer name the
//! next process comes back to, and the pending-first read above is what finds
//! it. This is exactly why [`afd_redis::outbound_consumer`] must not carry a
//! process id.

use afd_redis::{OutboundDelivery, OutboundQueue, OutboundReader};
use tokio_util::sync::CancellationToken;

use crate::poster::{Deliver, Posters, Verdict, deliver_with_retry};

/// How long one blocking read parks before it answers empty.
///
/// A bound rather than a timeout: `BLOCK 0` waits forever, and a read that
/// never returns is a task the supervisor cannot join even when it wins the
/// race — the select would be waiting on a future that is not going to
/// complete. Five seconds is long enough that an idle deployment issues twelve
/// commands a minute rather than the Zig's two hundred and forty, and short
/// enough that nothing waits on it: cancellation does not wait out the
/// interval, because the token is raced against the read rather than checked
/// between reads.
pub const BLOCK_INTERVAL: usize = 5_000;

/// The connector answer-delivery worker.
#[derive(Debug)]
pub struct Worker<S> {
    reader: OutboundReader,
    queue: OutboundQueue,
    posters: Posters<S>,
}

/// What one turn of the loop found.
enum Turn {
    /// A job to deliver.
    Job(Box<OutboundDelivery>),
    /// Nothing was waiting, or the read failed and the next turn will retry.
    Idle,
    /// The supervisor asked this task to stop.
    Stopped,
}

impl<S: Deliver> Worker<S> {
    /// Binds the worker to its own reader, the shared queue, and its posters.
    ///
    /// The reader is taken by value because it owns a connection this worker
    /// will park on — see [`afd_redis::Dedicated`].
    #[must_use]
    pub const fn new(reader: OutboundReader, queue: OutboundQueue, posters: Posters<S>) -> Self {
        Self {
            reader,
            queue,
            posters,
        }
    }

    /// Runs until the supervisor cancels `token`.
    ///
    /// Returns rather than panicking on a queue that will not answer: a Redis
    /// blip must not take the delivery path down for the life of the process,
    /// and the next turn of the loop re-reads. The one thing that ends this
    /// function is cancellation.
    pub async fn run(mut self, token: CancellationToken) {
        // Idempotent, and here rather than only at boot so a worker that starts
        // before the group exists — or after one is lost to a failover onto an
        // empty replica — heals itself instead of reading into a `NOGROUP`
        // forever.
        if let Err(failure) = self.queue.ensure_group().await {
            report("outbound_group_ensure_failed", &failure.into());
        }

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let consumer = self.reader.consumer().to_owned();
        tracing::debug!(consumer, event = "outbound_worker_started");

        loop {
            match self.next(&token).await {
                Turn::Job(job) => self.deliver_and_ack(&job, &token).await,
                Turn::Idle => {}
                Turn::Stopped => break,
            }
        }
        tracing::debug!(consumer, event = "outbound_worker_shutdown");
    }

    /// The next job, if there is one — pending first, then a blocking read.
    ///
    /// A read that FAILS answers [`Turn::Idle`] rather than propagating: the
    /// loop's next turn re-reads, and the blocking read's own interval is what
    /// keeps a failing Redis from being asked in a tight spin.
    async fn next(&mut self, token: &CancellationToken) -> Turn {
        // Before anything else, and before the token is consulted: a shutdown
        // that arrived while a previous job was delivering must not skip past
        // the pending list, because this read is the ONLY thing that finds what
        // the last process was handed.
        match self.reader.read_pending().await {
            Ok(Some(job)) => return Turn::Job(Box::new(job)),
            Ok(None) => {}
            Err(failure) => {
                report("outbound_read_pending_failed", &failure.into());
                return Turn::Idle;
            }
        }
        if token.is_cancelled() {
            return Turn::Stopped;
        }

        tokio::select! {
            // Biased so a token that is already cancelled wins deterministically
            // rather than by whichever future `select!` happened to poll first —
            // an unbiased race here would occasionally start a five-second park
            // during a shutdown that had already been requested.
            biased;
            () = token.cancelled() => Turn::Stopped,
            read = self.reader.read_blocking(BLOCK_INTERVAL) => match read {
                Ok(Some(job)) => Turn::Job(Box::new(job)),
                Ok(None) => Turn::Idle,
                Err(failure) => {
                    report("outbound_read_next_failed", &failure.into());
                    Turn::Idle
                }
            },
        }
    }

    /// Delivers one job with bounded retry, then acknowledges it.
    ///
    /// # Every terminal verdict acknowledges, including the exhausted one
    ///
    /// A job whose attempts ran out is acknowledged and logged, not left
    /// pending. Leaving it would redeliver it on the next turn, forever, at the
    /// head of a serial queue — one undeliverable answer would stop every
    /// answer behind it. The durable stream's job is to survive a CRASH, and a
    /// crash is precisely the case where the ack never runs.
    async fn deliver_and_ack(&self, job: &OutboundDelivery, token: &CancellationToken) {
        if deliver_with_retry(&self.posters, job, token).await == Verdict::Retryable {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let error_code = afd_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
            let provider = job.provider.as_str();
            let fleet_id = job.fleet_id.as_str();
            // A shutdown cut the retries short, so this is not an exhausted
            // budget — it is work this process is handing back. Left
            // UNACKNOWLEDGED on purpose: the entry stays in this consumer's
            // pending list, and the next process's pending-first read is what
            // picks it up. The "re-queues" half of Dimension 5.2, and the
            // reason the branch sits before the ack rather than after it.
            if token.is_cancelled() {
                tracing::info!(
                    provider,
                    fleet_id,
                    event = "outbound_delivery_requeued_at_shutdown"
                );
                return;
            }
            tracing::warn!(
                error_code,
                provider,
                fleet_id,
                event = "outbound_delivery_exhausted"
            );
        }
        if let Err(failure) = self.queue.ack(&job.id).await {
            // The delivery HAPPENED. What failed is the record of it, so the
            // job stays pending and will be delivered a second time — which is
            // why the whole path is at-least-once and the destination's own
            // thread is what a person reads.
            report("outbound_ack_failed", &failure.into());
        }
    }
}

/// Logs a failure the loop is choosing to continue past.
///
/// One site, so every swallowed failure is logged the same way and none is
/// swallowed silently — the failure mode `worker.zig`'s per-call `catch` blocks
/// each have to remember on their own.
fn report(event: &'static str, failure: &crate::Error) {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let error_code = failure.code().as_str();
    let reason = failure.to_string();
    tracing::warn!(error_code, reason, event);
}
