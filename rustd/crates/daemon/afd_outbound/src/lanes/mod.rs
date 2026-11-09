//! Per-destination delivery lanes under one in-flight ceiling.
//!
//! # Why the worker does not deliver what it claims
//!
//! A delivery is a vendor call with a deadline. A worker that delivered each
//! claimed job before claiming the next would let one destination that is
//! slow to answer hold every other workspace's answer behind it. So the worker
//! hands each job to the LANE for its destination and claims on.
//!
//! # What a lane promises
//!
//! One lane per destination, and each lane delivers serially, so two answers
//! to the same workspace leave in the order they were queued — the property
//! a person reading a thread notices when it breaks. Lanes are independent
//! of each other, so a slow destination holds only its own lane. What bounds
//! them together is the ceiling: at most [`IN_FLIGHT_DELIVERIES`] vendor
//! calls are in flight across every lane, which is what keeps a burst of
//! answers from opening a connection per workspace at once.
//!
//! A lane exists only while it holds work. It retires the moment its queue
//! is empty, holding the same map entry a dispatch takes to find it — the
//! map is sharded, so that is one shard's lock rather than the whole map's —
//! so a job never lands in a lane that is leaving: publication returns the
//! job and dispatch spawns a fresh lane, and since the old one drained everything
//! before it left, the order within the destination holds across the
//! hand-over.
//!
//! # How far a slow destination can reach
//!
//! A lane holds at most [`LANE_DEPTH`] jobs in memory, and Postgres claims at
//! most one head per lane at a time, so a lane rarely holds more than one.
//!
//! # A holder renews, and a lapsed holder stops
//!
//! A delivery renews its claim every [`CLAIM_RENEW`] while it runs. A renewal
//! that finds the claim gone means another holder may own the row, so the
//! delivery is abandoned unfinished rather than posting twice.
//!
//! # Cancellation gives the claim back
//!
//! A lane asked to stop finishes nothing new; a job whose delivery the
//! shutdown interrupted releases its claim at once, so another replica can
//! take it on its next claim pass.

use std::sync::Arc;

use afd_db::Db;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::mpsc;
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use std::time::Duration;

use crate::obligation::{self, AbandonReason, CLAIM_RENEW, CLAIM_TTL, Claimed};
use crate::poster::{Attempt, Deliver, Posters, Verdict, deliver_with_retry};

/// Delivery cycles an answer gets before it is abandoned.
pub const MAX_DELIVERY_CYCLES: i64 = 12;

/// How long a retryable answer waits before it is due again.
pub const CYCLE_BACKOFF: Duration = Duration::from_secs(30);

mod dispatch;
mod retire;

/// How many vendor calls may be in flight at once, across every lane.
pub const IN_FLIGHT_DELIVERIES: usize = 8;

/// How many jobs one lane holds before a dispatch to it waits.
pub const LANE_DEPTH: usize = 32;

/// The structured events this module emits, named once each (RULE UFS).
const EVENT_REQUEUED_AT_SHUTDOWN: &str = "outbound_delivery_requeued_at_shutdown";
const EVENT_DELIVERY_EXHAUSTED: &str = "outbound_delivery_exhausted";
const EVENT_DELIVERED: &str = "outbound_delivery_delivered";
const EVENT_STAMP_FAILED: &str = "outbound_obligation_stamp_failed";
const EVENT_ATTEMPT_COUNT_FAILED: &str = "outbound_obligation_attempt_failed";
const EVENT_RELEASE_FAILED: &str = "outbound_claim_release_failed";
const EVENT_RENEW_FAILED: &str = "outbound_claim_renew_failed";
const EVENT_CLAIM_LAPSED: &str = "outbound_claim_lapsed";

/// Where an answer goes: one provider's workspace.
///
/// The unit of ordering and of isolation. Two answers to one workspace must
/// arrive in order; two workspaces must not wait on each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Destination {
    provider: String,
    workspace_id: String,
}

impl Destination {
    /// The destination a job is addressed to.
    #[must_use]
    pub fn of(job: &Claimed) -> Self {
        Self {
            provider: job.outgoing.provider.clone(),
            workspace_id: job.outgoing.workspace_id.clone().into(),
        }
    }
}

/// A job as the lanes carry it: boxed, because the answer text is the bulk
/// of it and a channel moves the box, not the text.
type Job = Box<Claimed>;

/// The lanes, over one set of posters and the obligation ledger.
///
/// Cheap to clone: every clone shares the same lanes, ceiling and tracker.
#[derive(Debug)]
pub struct Lanes<S> {
    inner: Arc<Inner<S>>,
}

impl<S> Clone for Lanes<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[derive(Debug)]
struct Inner<S> {
    posters: Posters<S>,
    /// The obligation ledger, for stamping what a destination actually took.
    database: Db,
    permits: Semaphore,
    /// Protects lane membership and publication against retirement. Capacity
    /// is reserved before taking a guard; no guard crosses an await.
    lanes: DashMap<Destination, mpsc::Sender<Job>>,
    tasks: TaskTracker,
    token: CancellationToken,
    /// How often a delivery renews its claim.
    renew: Duration,
    /// Rung after every finished job: a lane's next row may now be claimable.
    finished: Notify,
}

impl<S: Deliver + 'static> Lanes<S> {
    /// Lanes delivering through `posters`, stamping obligations in
    /// `database`, and stopping on `token`.
    #[must_use]
    pub fn new(posters: Posters<S>, database: Db, token: CancellationToken) -> Self {
        Self::renewing_every(posters, database, token, CLAIM_RENEW)
    }

    /// [`Lanes::new`], renewing each delivery's claim every `renew`.
    #[must_use]
    pub fn renewing_every(
        posters: Posters<S>,
        database: Db,
        token: CancellationToken,
        renew: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                posters,
                database,
                permits: Semaphore::new(IN_FLIGHT_DELIVERIES),
                lanes: DashMap::new(),
                tasks: TaskTracker::new(),
                token,
                renew,
                finished: Notify::new(),
            }),
        }
    }

    /// Resolves after the next job finishes, however it ended.
    pub async fn finished(&self) {
        self.inner.finished.notified().await;
    }

    /// How many lanes currently hold work.
    #[must_use]
    pub fn active(&self) -> usize {
        self.inner.lanes.len()
    }

    /// Waits for every lane to finish the job in hand and stop.
    ///
    /// Called after the token is cancelled, which is what makes it bounded:
    /// no lane starts another job once it is, and none starts another retry.
    pub async fn drain(self) {
        self.inner.tasks.close();
        self.inner.tasks.wait().await;
    }
}

impl<S: Deliver + 'static> Inner<S> {
    /// The sender for `destination`'s lane, spawning the lane if it has none.
    ///
    /// The entry holds its shard across the look and the insert, so two
    /// dispatches to one new destination spawn one lane rather than two —
    /// the check-then-act a `get` followed by an `insert` would not give.
    fn lane_for(self: &Arc<Self>, destination: &Destination) -> mpsc::Sender<Job> {
        match self.lanes.entry(destination.clone()) {
            Entry::Occupied(lane) => lane.get().clone(),
            Entry::Vacant(slot) => {
                let (sender, receiver) = mpsc::channel(LANE_DEPTH);
                self.tasks
                    .spawn(Arc::clone(self).run_lane(destination.clone(), receiver));
                slot.insert(sender).clone()
            }
        }
    }

    /// One delivery cycle of a claimed job; the worker hears when it ends.
    ///
    /// # The cycle is counted before it is run
    ///
    /// The count is recorded first, while failure is still one of the endings.
    /// Counting after a verdict would mean counting only the verdicts that
    /// reached the counter, which is how `attempt_count` came to hold a tally
    /// of successes: the destination that refuses an answer forever is the row
    /// worth finding, and it is the one a success counter never records.
    async fn deliver(&self, job: &Claimed) {
        self.deliver_once(job).await;
        self.finished.notify_one();
    }

    /// One delivery cycle of a claimed job, renewing the claim while it runs.
    async fn deliver_once(&self, job: &Claimed) {
        // A row naming no connector this build knows has no poster to try.
        if afd_connector::Provider::parse(&job.outgoing.provider).is_none() {
            crate::abandon::delivery(&self.database, job, AbandonReason::Unaddressable).await;
            return;
        }
        let attempts = self.count_cycle(job).await;
        let opening = Attempt::opening(attempts);
        let verdict = tokio::select! {
            verdict = deliver_with_retry(&self.posters, &job.outgoing, &self.token, opening) => Ok(verdict),
            lost = self.renewing(job) => Err(lost),
        };
        let verdict = match verdict {
            Ok(verdict) => verdict,
            // The claim lapsed, or may have: another holder may be posting it.
            Err(lost) => {
                Self::lapsed(job, lost);
                return;
            }
        };
        match verdict {
            Verdict::Delivered => self.stamp_delivered(job, attempts).await,
            Verdict::Permanent => {
                crate::abandon::delivery(&self.database, job, AbandonReason::Refused).await;
            }
            Verdict::Retryable => self.retry_later(job, attempts).await,
        }
    }

    /// Renews `job`'s claim until it is lost, and answers how: `renew` when
    /// another holder took the row, `renew_unanswered` when no renewal has
    /// landed for so long that the claim may lapse before the next one.
    /// Never returns while the claim is known to hold.
    async fn renewing(&self, job: &Claimed) -> &'static str {
        let mut held_at = tokio::time::Instant::now();
        loop {
            tokio::time::sleep(self.renew).await;
            match obligation::renew(&self.database, job).await {
                Ok(true) => held_at = tokio::time::Instant::now(),
                Ok(false) => return "renew",
                Err(failure) => {
                    crate::worker::report(EVENT_RENEW_FAILED, &failure);
                    // Stop before the claim can lapse: a replica may retake
                    // the row, and two holders would both post it.
                    if held_at.elapsed() + self.renew >= CLAIM_TTL {
                        return "renew_unanswered";
                    }
                }
            }
        }
    }

    /// A retryable cycle: give the claim back, due again later, or abandon it
    /// once its cycles are spent. A shutdown gives it back due at once.
    async fn retry_later(&self, job: &Claimed, attempts: Option<i64>) {
        let provider = job.outgoing.provider.as_str();
        let fleet_id = job.outgoing.fleet_id.as_str();
        if self.token.is_cancelled() {
            tracing::info!(
                provider,
                fleet_id,
                attempts,
                event = EVENT_REQUEUED_AT_SHUTDOWN
            );
            self.release(job, Duration::ZERO).await;
            return;
        }
        let error_code = af_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
        tracing::warn!(
            error_code,
            provider,
            fleet_id,
            attempts,
            event = EVENT_DELIVERY_EXHAUSTED
        );
        if attempts.is_some_and(|cycles| cycles >= MAX_DELIVERY_CYCLES) {
            crate::abandon::delivery(&self.database, job, AbandonReason::CyclesExhausted).await;
            return;
        }
        self.release(job, CYCLE_BACKOFF).await;
    }

    async fn release(&self, job: &Claimed, after: Duration) {
        match obligation::release(&self.database, job, after).await {
            Ok(true) => {}
            Ok(false) => Self::lapsed(job, "release"),
            Err(failure) => crate::worker::report(EVENT_RELEASE_FAILED, &failure),
        }
    }

    /// A holder's write matched nothing: its claim had lapsed.
    fn lapsed(job: &Claimed, write: &'static str) {
        let provider = job.outgoing.provider.as_str();
        let fleet_id = job.outgoing.fleet_id.as_str();
        let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(
            error_code,
            provider,
            fleet_id,
            write,
            event = EVENT_CLAIM_LAPSED
        );
    }

    async fn count_cycle(&self, job: &Claimed) -> Option<i64> {
        match crate::obligation::count_attempt(&self.database, job).await {
            Ok(counted) => counted,
            Err(failure) => {
                crate::worker::report(EVENT_ATTEMPT_COUNT_FAILED, &failure);
                None
            }
        }
    }

    /// Records that a destination took this answer.
    ///
    /// A failure here leaves the row claimed and unstamped; the claim lapses
    /// and the next holder posts it again. That costs a duplicate message in a
    /// thread; the opposite error — marking delivered what was not — loses the
    /// answer silently.
    async fn stamp_delivered(&self, job: &Claimed, attempts: Option<i64>) {
        match crate::obligation::stamp_delivered(&self.database, job).await {
            Ok(true) => {}
            Ok(false) => {
                Self::lapsed(job, "stamp");
                return;
            }
            Err(failure) => {
                crate::worker::report(EVENT_STAMP_FAILED, &failure);
                return;
            }
        }
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let provider = job.outgoing.provider.as_str();
        let fleet_id = job.outgoing.fleet_id.as_str();
        // The turn this answer belongs to. Every other layer stamps it, and a
        // delivery line that names only the fleet cannot be joined back to the
        // turn a reader is tracing — nor tell two turns of one fleet apart.
        let agentsfleet_event_id = job.outgoing.event_id.as_str();
        tracing::debug!(
            provider,
            fleet_id,
            agentsfleet_event_id,
            attempts,
            event = EVENT_DELIVERED
        );
    }
}
