//! What one lane does between jobs, and how it leaves.
//!
//! Publication in `dispatch.rs` and retirement share the destination entry's
//! guard. Capacity reservations alone never transfer ownership of a job.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;

use dashmap::mapref::entry::Entry;

use super::{Destination, Inner, Job};
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
    /// Publication also takes this entry's guard: either it already queued
    /// the job and the second look sees it, or retirement removes the lane
    /// first and publication returns the job for a fresh lane. A reserved
    /// permit cannot enqueue after this second look while the guard is held.
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
        let Entry::Occupied(lane) = self.lanes.entry(destination.clone()) else {
            return None;
        };
        if let Ok(job) = jobs.try_recv() {
            return Some(job);
        }
        jobs.close();
        lane.remove();
        None
    }
}
