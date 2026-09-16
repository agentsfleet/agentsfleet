//! What one lane does between jobs, and how it leaves.
//!
//! Split from `lanes.rs` at the file-length cap: the retirement path carries
//! more prose than code, because the two interleavings it rules out are what
//! the module's ordering and at-least-once promises rest on.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;

use dashmap::mapref::entry::Entry;

use super::{Destination, Inner, Job, LANE_DEPTH};
use crate::poster::Deliver;

impl<S: Deliver + 'static> Inner<S> {
    /// One lane: deliver what is queued for `destination`, in order, then
    /// retire.
    pub(super) async fn run_lane(
        self: Arc<Self>,
        destination: Destination,
        mut jobs: mpsc::Receiver<Job>,
    ) {
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
    /// The second look happens holding this lane's map entry, which is what a
    /// dispatch takes to find it: either the dispatch already queued its job
    /// and this sees it, or this has retired the lane and the dispatch will
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
    /// entry gone, and spawns a fresh lane.
    ///
    /// # Why the hand-over happens holding the entry
    ///
    /// A send can still land between the `try_recv` that found the lane empty
    /// and the close on the next line — the sender takes no lock. Handing that
    /// job to a fresh lane AFTER releasing the entry would deliver it out of
    /// ORDER: the next job for the destination finds no lane, spawns its own,
    /// and overtakes the one still being handed over. So the fresh lane is
    /// loaded and put in this entry's place while the entry is held, and a
    /// dispatch arriving next queues behind what was rescued — which is the
    /// order the destination was promised.
    fn take_or_retire(
        self: &Arc<Self>,
        destination: &Destination,
        jobs: &mut mpsc::Receiver<Job>,
    ) -> Option<Job> {
        match jobs.try_recv() {
            Ok(job) => return Some(job),
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => {}
        }
        // Already gone: this lane retired on an earlier pass, or a dispatch
        // respawned the destination and a fresh lane owns it.
        let Entry::Occupied(mut lane) = self.lanes.entry(destination.clone()) else {
            return None;
        };
        if let Ok(job) = jobs.try_recv() {
            return Some(job);
        }
        jobs.close();
        let stragglers: Vec<Job> = std::iter::from_fn(|| jobs.try_recv().ok()).collect();
        if stragglers.is_empty() {
            lane.remove();
            return None;
        }
        drop(lane.insert(self.hand_over(destination, stragglers)));
        None
    }

    /// A fresh lane for `destination`, already holding `stragglers` in order.
    ///
    /// Synchronous on purpose: it runs while the map entry is held, and an
    /// await there would hold a shard across a scheduling point. The jobs came
    /// out of a channel of [`LANE_DEPTH`] and go into another of that depth,
    /// and nothing else holds this sender yet, so every `try_send` has room.
    fn hand_over(
        self: &Arc<Self>,
        destination: &Destination,
        stragglers: Vec<Job>,
    ) -> mpsc::Sender<Job> {
        let (sender, receiver) = mpsc::channel(LANE_DEPTH);
        self.tasks
            .spawn(Arc::clone(self).run_lane(destination.clone(), receiver));
        for straggler in stragglers {
            if let Err(rejected) = sender.try_send(straggler) {
                // Unreachable while the room above holds. Said out loud rather
                // than dropped: the entry stays pending on the stream, so the
                // next process redelivers it, and this is the only line in THIS
                // process that says an answer was never handed anywhere.
                let job = rejected.into_inner();
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                let provider = job.provider.as_str();
                let fleet_id = job.fleet_id.as_str();
                tracing::error!(
                    error_code,
                    provider,
                    fleet_id,
                    event = "outbound_lane_handover_refused"
                );
            }
        }
        sender
    }
}
