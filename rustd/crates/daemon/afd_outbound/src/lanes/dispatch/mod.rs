//! Capacity reservation and job publication for one destination.

use std::sync::atomic::Ordering;

use dashmap::DashMap;
use tokio::sync::mpsc;

use super::{Destination, Job, Lanes};
use crate::Result;
use crate::poster::Deliver;

impl<S: Deliver + 'static> Lanes<S> {
    /// Queues `job` on its destination's lane, spawning the lane if needed.
    ///
    /// Returns once the lane has taken the job. A lane holding [`super::LANE_DEPTH`]
    /// jobs makes the caller wait for capacity before reading the stream again.
    pub async fn dispatch(&self, mut job: Job) {
        let destination = Destination::of(&job);
        loop {
            let lane = self.inner.lane_for(&destination);
            let Ok(permit) = lane.reserve().await else {
                continue;
            };
            // Counted before the lane can take it, so the lane's finish never
            // runs ahead of the count; no await between here and the publish.
            self.inner.held.fetch_add(1, Ordering::AcqRel);
            match publish(&self.inner.lanes, &destination, &lane, permit, job) {
                Ok(()) => return,
                Err(mpsc::error::SendError(returned)) => {
                    self.inner.held.fetch_sub(1, Ordering::AcqRel);
                    job = returned;
                }
            }
        }
    }
}

/// Publishes a reserved job, returning its ownership if the lane retired.
///
/// A permit can still send after its receiver closes. Retirement takes this
/// same map entry, so channel identity and publication must share one guard.
pub(super) fn publish(
    lanes: &DashMap<Destination, mpsc::Sender<Job>>,
    destination: &Destination,
    sender: &mpsc::Sender<Job>,
    permit: mpsc::Permit<'_, Job>,
    job: Job,
) -> Result<(), mpsc::error::SendError<Job>> {
    let Some(current) = lanes.get(destination) else {
        return Err(mpsc::error::SendError(job));
    };
    if !current.same_channel(sender) {
        return Err(mpsc::error::SendError(job));
    }
    permit.send(job);
    drop(current);
    Ok(())
}

#[cfg(test)]
mod tests;
