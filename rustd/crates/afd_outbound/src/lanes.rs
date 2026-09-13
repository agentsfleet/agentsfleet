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
//! is empty, under the same lock a dispatch takes to find it, so a job never
//! lands in a lane that is leaving: the send fails and the dispatch spawns a
//! fresh lane, and since the old one drained everything before it left, the
//! order within the destination holds across the hand-over.
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

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use afd_datastore::{OutboundDelivery, OutboundQueue};
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::poster::{Deliver, Posters, Verdict, deliver_with_retry};

/// How many vendor calls may be in flight at once, across every lane.
pub const IN_FLIGHT_DELIVERIES: usize = 8;

/// How many jobs one lane holds before a dispatch to it waits.
pub const LANE_DEPTH: usize = 32;

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
    permits: Semaphore,
    lanes: Mutex<HashMap<Destination, mpsc::Sender<Job>>>,
    tasks: TaskTracker,
    token: CancellationToken,
}

impl<S: Deliver + 'static> Lanes<S> {
    /// Lanes delivering through `posters`, acknowledging through `queue`, and
    /// stopping on `token`.
    #[must_use]
    pub fn new(posters: Posters<S>, queue: OutboundQueue, token: CancellationToken) -> Self {
        Self {
            inner: Arc::new(Inner {
                posters,
                queue,
                permits: Semaphore::new(IN_FLIGHT_DELIVERIES),
                lanes: Mutex::new(HashMap::new()),
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
        self.inner.lock_lanes().len()
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
    /// The lane map. Poisoning cannot happen — nothing that runs under this
    /// lock can panic — and recovering the guard is the honest answer if it
    /// somehow did, rather than propagating a panic into every later dispatch.
    fn lock_lanes(&self) -> MutexGuard<'_, HashMap<Destination, mpsc::Sender<Job>>> {
        self.lanes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The sender for `destination`'s lane, spawning the lane if it has none.
    fn lane_for(self: &Arc<Self>, destination: &Destination) -> mpsc::Sender<Job> {
        let mut lanes = self.lock_lanes();
        if let Some(lane) = lanes.get(destination) {
            return lane.clone();
        }
        let (sender, receiver) = mpsc::channel(LANE_DEPTH);
        lanes.insert(destination.clone(), sender.clone());
        self.tasks
            .spawn(Arc::clone(self).run_lane(destination.clone(), receiver));
        sender
    }

    /// One lane: deliver what is queued for `destination`, in order, then
    /// retire.
    async fn run_lane(self: Arc<Self>, destination: Destination, mut jobs: mpsc::Receiver<Job>) {
        while let Some(job) = self.take_or_retire(&destination, &mut jobs) {
            // Between jobs, and before a permit is waited for: a job taken
            // here and not delivered stays unacknowledged, which is the
            // hand-off to the next process.
            if self.token.is_cancelled() {
                return;
            }
            let permit = tokio::select! {
                biased;
                () = self.token.cancelled() => return,
                permit = self.permits.acquire() => permit,
            };
            // The semaphore is never closed, so this is the permit; a closed
            // one would mean the ceiling itself is gone, and stopping is the
            // only honest response to that.
            let Ok(_permit) = permit else { return };
            self.deliver_and_ack(&job).await;
        }
    }

    /// The next queued job, or `None` after retiring the lane.
    ///
    /// The second look happens under the lane lock, which is the same lock a
    /// dispatch takes to find this lane: either the dispatch already queued
    /// its job and this sees it, or this has removed the lane and the
    /// dispatch will spawn a fresh one. There is no third interleaving.
    fn take_or_retire(
        &self,
        destination: &Destination,
        jobs: &mut mpsc::Receiver<Job>,
    ) -> Option<Job> {
        match jobs.try_recv() {
            Ok(job) => Some(job),
            Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => {
                let mut lanes = self.lock_lanes();
                jobs.try_recv().ok().or_else(|| {
                    lanes.remove(destination);
                    None
                })
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
    async fn deliver_and_ack(&self, job: &OutboundDelivery) {
        if deliver_with_retry(&self.posters, job, &self.token).await == Verdict::Retryable {
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
            crate::worker::report("outbound_ack_failed", &failure.into());
        }
    }
}
