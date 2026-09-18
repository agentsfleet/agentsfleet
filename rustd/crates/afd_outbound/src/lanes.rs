//! Per-destination delivery lanes under one in-flight ceiling.
//!
//! # Why the worker does not deliver what it reads
//!
//! The outbound stream is one stream, read in order, and a delivery is a
//! vendor call with a deadline. A worker that delivered each job before
//! reading the next would let one destination that is slow to answer — a
//! workspace whose Slack is rate-limiting, a channel behind a timed-out
//! proxy — hold every answer queued behind it for every other workspace. So
//! the worker hands each job to the LANE for its destination and reads on.
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
//! so a job never lands in a lane that is leaving: the send fails and the
//! dispatch spawns a fresh lane, and since the old one drained everything
//! before it left, the order within the destination holds across the
//! hand-over.
//!
//! # How far a slow destination can reach
//!
//! A lane holds at most [`LANE_DEPTH`] jobs in memory. Past that the dispatch
//! waits, which is the stream buffering on the destination's behalf — the
//! entries are already read into this consumer's pending list and lose
//! nothing by waiting. The bound is stated rather than hidden: a destination
//! that will not take a job stalls unrelated answers only after that many of
//! its own have piled up, and the reader resumes the moment one drains.
//!
//! # Cancellation stops lanes between jobs, never inside one
//!
//! The rule the worker already keeps. A lane asked to stop finishes the job
//! in hand — one vendor deadline, since the retry loop stops starting new
//! attempts — and leaves everything still queued unacknowledged. Those
//! entries stay pending under this consumer's name, and the next process's
//! pending-first read is what delivers them.

use std::sync::Arc;

use afd_db::Db;
use afd_dragonfly::{OutboundDelivery, OutboundQueue};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::poster::{Deliver, Posters, Verdict, deliver_with_retry};

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
const EVENT_ACK_FAILED: &str = "outbound_ack_failed";

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
    pub fn of(job: &OutboundDelivery) -> Self {
        Self {
            provider: job.provider.clone(),
            workspace_id: job.workspace_id.clone(),
        }
    }
}

/// A job as the lanes carry it: boxed, because the answer text is the bulk
/// of it and a channel moves the box, not the text.
type Job = Box<OutboundDelivery>;

/// The lanes, over one set of posters and one queue to acknowledge through.
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
    queue: OutboundQueue,
    /// The obligation ledger, for stamping what a destination actually took.
    database: Db,
    permits: Semaphore,
    lanes: DashMap<Destination, mpsc::Sender<Job>>,
    tasks: TaskTracker,
    token: CancellationToken,
}

impl<S: Deliver + 'static> Lanes<S> {
    /// Lanes delivering through `posters`, acknowledging through `queue`,
    /// stamping obligations in `database`, and stopping on `token`.
    #[must_use]
    pub fn new(
        posters: Posters<S>,
        queue: OutboundQueue,
        database: Db,
        token: CancellationToken,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                posters,
                queue,
                database,
                permits: Semaphore::new(IN_FLIGHT_DELIVERIES),
                lanes: DashMap::new(),
                tasks: TaskTracker::new(),
                token,
            }),
        }
    }

    /// Queues `job` on the lane for its destination, spawning the lane if
    /// none holds work for that destination.
    ///
    /// Returns once the lane has taken the job, which is at once unless the
    /// lane already holds [`LANE_DEPTH`] — then this waits for it to drain
    /// one, and the caller's read of the stream waits with it.
    pub async fn dispatch(&self, mut job: Job) {
        loop {
            let lane = self.inner.lane_for(&Destination::of(&job));
            match lane.send(job).await {
                Ok(()) => return,
                // The lane retired between the lookup and the send. It left
                // with an empty queue, so re-dispatching spawns a fresh one
                // behind everything it delivered.
                Err(mpsc::error::SendError(returned)) => job = returned,
            }
        }
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

    /// Delivers one job with bounded retry, then acknowledges it.
    ///
    /// # Every terminal verdict acknowledges, including the exhausted one
    ///
    /// A job whose attempts ran out is acknowledged and logged, not left
    /// pending. Leaving it would redeliver it on the next pass, forever, at
    /// the head of its destination's lane — one undeliverable answer would
    /// stop every answer to that workspace behind it. The durable stream's
    /// job is to survive a CRASH, and a crash is precisely the case where the
    /// ack never runs.
    ///
    /// # The cycle is counted before it is run
    ///
    /// The count is recorded first, while failure is still one of the endings.
    /// Counting after a verdict would mean counting only the verdicts that
    /// reached the counter, which is how `attempt_count` came to hold a tally
    /// of successes: the destination that refuses an answer forever is the row
    /// worth finding, and it is the one a success counter never records.
    async fn deliver_and_ack(&self, job: &OutboundDelivery) {
        let attempts = self.count_cycle(job).await;
        let verdict = deliver_with_retry(&self.posters, job, &self.token).await;
        if verdict == Verdict::Delivered {
            self.stamp_delivered(job, attempts).await;
        }
        if verdict == Verdict::Retryable {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let error_code = afd_core::error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
            let provider = job.provider.as_str();
            let fleet_id = job.fleet_id.as_str();
            // A shutdown cut the retries short, so this is not an exhausted
            // budget — it is work this process is handing back. Left
            // UNACKNOWLEDGED on purpose: the entry stays in this consumer's
            // pending list, and the next process's pending-first read is what
            // picks it up.
            if self.token.is_cancelled() {
                tracing::info!(
                    provider,
                    fleet_id,
                    attempts,
                    event = EVENT_REQUEUED_AT_SHUTDOWN
                );
                return;
            }
            tracing::warn!(
                error_code,
                provider,
                fleet_id,
                attempts,
                event = EVENT_DELIVERY_EXHAUSTED
            );
        }
        if let Err(failure) = self.queue.ack(&job.id).await {
            // The delivery HAPPENED. What failed is the record of it, so the
            // job stays pending and will be delivered a second time — which is
            // why the whole path is at-least-once and the destination's own
            // thread is what a person reads.
            crate::worker::report(EVENT_ACK_FAILED, &failure.into());
        }
    }

    /// Records this delivery cycle against its obligation, and answers the
    /// count the write produced.
    ///
    /// `None` covers both of the ways there is no number to report: a row that
    /// was already delivered, so a duplicate queue entry counts nothing; and a
    /// database that would not answer. The second is logged and the delivery
    /// goes ahead regardless — the answer is owed to a person, and losing it
    /// because the bookkeeping failed is the one outcome worth nothing to
    /// anybody. The telemetry then under-reports, which it says by carrying no
    /// count rather than by carrying a wrong one.
    async fn count_cycle(&self, job: &OutboundDelivery) -> Option<i64> {
        match crate::obligation::count_attempt(
            &self.database,
            job.fleet_id.as_str(),
            job.event_id.as_str(),
            afd_core::clock::now(),
        )
        .await
        {
            Ok(counted) => counted,
            Err(failure) => {
                crate::worker::report(EVENT_ATTEMPT_COUNT_FAILED, &failure);
                None
            }
        }
    }

    /// Records that a destination took this answer.
    ///
    /// Stamped BEFORE the acknowledgement, because the two record different
    /// facts and only this one says a person received anything. The ack fires
    /// for an EXHAUSTED job too, so an ack-time stamp would mark undeliverable
    /// answers delivered.
    ///
    /// A failure here leaves the row receipted and unstamped, which the
    /// recovery scan reads as "queued and never received" and re-offers. That
    /// costs a duplicate message in a thread; the opposite error — marking
    /// delivered what was not — loses the answer silently.
    async fn stamp_delivered(&self, job: &OutboundDelivery, attempts: Option<i64>) {
        if let Err(failure) = crate::obligation::stamp_delivered(
            &self.database,
            job.fleet_id.as_str(),
            job.event_id.as_str(),
            afd_core::clock::now(),
        )
        .await
        {
            crate::worker::report(EVENT_STAMP_FAILED, &failure);
            return;
        }
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let provider = job.provider.as_str();
        let fleet_id = job.fleet_id.as_str();
        tracing::debug!(provider, fleet_id, attempts, event = EVENT_DELIVERED);
    }
}
