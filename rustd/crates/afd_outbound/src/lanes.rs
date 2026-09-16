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
        self.rescue_stragglers(&mut jobs).await;
    }

    /// Re-dispatches anything that reached this lane as it was retiring.
    ///
    /// [`Self::take_or_retire`] closes the channel inside the map entry, so no
    /// send can land after that point. A send can still land between the
    /// `try_recv` that found the lane empty and the close on the next line — the
    /// sender holds a clone and takes no lock — and that job would otherwise
    /// go out with this receiver when the task returns. It is instead handed
    /// to a fresh lane.
    ///
    /// Ordinarily this drains nothing: the window is two statements wide. It
    /// is not optional for that reason. A job dropped here is an answer this
    /// process never delivers, and while the entry stays pending on the stream
    /// — so the next process redelivers it and the path is still at-least-once
    /// — nothing in THIS process ever says so.
    async fn rescue_stragglers(self: &Arc<Self>, jobs: &mut mpsc::Receiver<Job>) {
        while let Ok(mut straggler) = jobs.try_recv() {
            if self.token.is_cancelled() {
                // Shutting down: leave it unacknowledged, which is the
                // hand-off to the next process this module already relies on.
                return;
            }
            // `Lanes::dispatch`'s loop, from the inside: this lane's own
            // channel is closed, so `lane_for` answers a fresh one.
            loop {
                let lane = self.lane_for(&Destination::of(&straggler));
                match lane.send(straggler).await {
                    Ok(()) => break,
                    Err(mpsc::error::SendError(returned)) => straggler = returned,
                }
            }
        }
    }

    /// The next queued job, or `None` after retiring the lane.
    ///
    /// The second look happens holding this lane's map entry, which is what a
    /// dispatch takes to find it: either the dispatch already queued its job
    /// and this sees it, or this has removed the lane and the dispatch will
    /// spawn a fresh one.
    ///
    /// # The interleaving the close exists for
    ///
    /// A third one was reachable, and it lost a job. [`Inner::lane_for`] hands
    /// out a CLONE of the sender and then releases the entry, so a dispatch can
    /// be holding a live sender while this runs. Removing the entry does not
    /// invalidate that clone, and the receiver stays alive until `run_lane`
    /// returns — so a `send` landing in between SUCCEEDED, into a buffer that
    /// was about to be dropped with the task. The answer was never delivered
    /// and nothing said so.
    ///
    /// Closing the channel first is what makes that impossible: after it, the
    /// stale clone's `send` fails, `dispatch` re-enters its loop, finds the
    /// entry gone, and spawns a fresh lane. The close happens while the entry
    /// is held, so a dispatch cannot be between `lane_for` and the map at the
    /// same moment. Anything already buffered is rescued by
    /// [`Self::rescue_stragglers`].
    fn take_or_retire(
        &self,
        destination: &Destination,
        jobs: &mut mpsc::Receiver<Job>,
    ) -> Option<Job> {
        match jobs.try_recv() {
            Ok(job) => Some(job),
            Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => match self.lanes.entry(destination.clone()) {
                Entry::Occupied(lane) => jobs.try_recv().ok().or_else(|| {
                    jobs.close();
                    lane.remove();
                    None
                }),
                // Already gone: this lane retired on an earlier pass, or a
                // dispatch respawned the destination and a fresh lane owns it.
                Entry::Vacant(_no_lane) => None,
            },
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
        let verdict = deliver_with_retry(&self.posters, job, &self.token).await;
        if verdict == Verdict::Delivered {
            // Stamped BEFORE the acknowledgement, because the two record
            // different facts and only this one says a person received
            // anything. The ack below fires for an EXHAUSTED job too, so an
            // ack-time stamp would mark undeliverable answers delivered.
            //
            // A failure here leaves the row receipted and unstamped, which the
            // recovery scan reads as "queued and never received" and re-offers.
            // That costs a duplicate message in a thread; the opposite error —
            // marking delivered what was not — loses the answer silently.
            if let Err(failure) = crate::obligation::stamp_delivered(
                &self.database,
                job.fleet_id.as_str(),
                job.event_id.as_str(),
                afd_core::clock::now(),
            )
            .await
            {
                crate::worker::report("outbound_obligation_stamp_failed", &failure);
            }
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
