//! What one lane does between jobs, and how it leaves.
//!
//! Publication in `dispatch.rs` and retirement share the destination entry's
//! guard. Capacity reservations alone never transfer ownership of a job.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;

use dashmap::mapref::entry::Entry;

use super::{Destination, Inner, Job};
use crate::obligation::Claimed;
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
            let stop = self.run_job(&job).await;
            self.done();
            if stop {
                return;
            }
        }
    }

    /// Delivers one taken job; `true` when the lane must stop.
    async fn run_job(&self, job: &Claimed) -> bool {
        // Between jobs, and before a permit is waited for: a job taken here
        // and not delivered gives its claim back due at once, so another
        // replica takes it on its next claim pass.
        if self.token.is_cancelled() {
            self.release(job, Duration::ZERO).await;
            return true;
        }
        let permit = tokio::select! {
            biased;
            () = self.token.cancelled() => {
                self.release(job, Duration::ZERO).await;
                return true;
            }
            permit = self.permits.acquire() => permit,
        };
        // The semaphore is never closed, so this is the permit; a closed one
        // would mean the ceiling itself is gone, and stopping is the only
        // honest response to that.
        let Ok(_permit) = permit else { return true };
        self.deliver(job).await;
        false
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

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a failed test precondition must stop the test"
    )]

    use std::future::ready;
    use std::sync::Arc;
    use std::time::Duration;

    use af_core::id::Uuid7;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::lanes::{Destination, LANE_DEPTH, Lanes};
    use crate::obligation::{Claimed, Outgoing};
    use crate::poster::{Deliver, Posters, Verdict};

    #[derive(Debug)]
    struct UnusedPoster;

    impl Deliver for UnusedPoster {
        fn deliver(&self, _job: &Outgoing) -> impl Future<Output = Verdict> + Send {
            ready(Verdict::Delivered)
        }
    }

    fn id(text: &str) -> Uuid7 {
        Uuid7::parse(text).expect("a fixture identifier is a canonical UUIDv7")
    }

    /// A job published while the lane makes its retiring look is taken by
    /// the lane, never dropped with the channel it was about to close.
    #[tokio::test]
    async fn a_job_published_during_retirement_is_taken_not_lost() {
        let lanes = Lanes::new(
            Posters {
                slack: UnusedPoster,
            },
            afd_db::test_util::unreachable_db(),
            CancellationToken::new(),
        );
        let event = id("0199a0b0-0002-7000-8000-000000000001");
        let job = Box::new(Claimed {
            id: afd_db::test_util::mint_id(),
            token: afd_db::test_util::mint_id(),
            outgoing: Outgoing {
                provider: "slack".to_owned(),
                destination: "thread".to_owned(),
                workspace_id: id("0199a0b0-0002-7000-8000-000000000002"),
                fleet_id: id("0199a0b0-0002-7000-8000-000000000003"),
                event_id: event.clone(),
                answer: "answer".to_owned(),
            },
        });
        let destination = Destination::of(&job);
        let (sender, mut receiver) = mpsc::channel(LANE_DEPTH);
        lanes.inner.lanes.insert(destination.clone(), sender);
        let inner = Arc::clone(&lanes.inner);

        let taken = std::thread::scope(|scope| {
            // Publication's guard: the lane's first look finds its queue
            // empty, then waits here for the entry it would remove.
            let publishing = inner
                .lanes
                .get(&destination)
                .expect("the lane is registered");
            let retiring = scope.spawn(|| inner.take_or_retire(&destination, &mut receiver));
            std::thread::sleep(Duration::from_millis(50));
            publishing.try_send(job).expect("the lane has room");
            drop(publishing);
            retiring.join().expect("the retiring look completes")
        });

        assert_eq!(taken.map(|job| job.outgoing.event_id), Some(event));
        assert!(
            lanes.inner.lanes.contains_key(&destination),
            "a lane that found work does not retire"
        );
    }
}
