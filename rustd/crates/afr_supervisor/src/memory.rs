//! A fleet's durable memory: read at lease start, written back fenced.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::{MemoryDelta, MemoryPushRequest};
use afr_agent::Checkpoint;

use crate::client::{Body, ControlPlane, retrying};
use crate::error::Result;

/// The event a checkpoint that was not written logs under.
const EVENT_CHECKPOINT_FAILED: &str = "memory_checkpoint_failed";

/// Reads the fleet's memory, retrying a blip.
///
/// # Errors
/// A refusal, or a retryable failure that outlasted its attempts.
pub(crate) async fn hydrate(plane: &ControlPlane, fleet_id: &Uuid7) -> Result<Body> {
    retrying(|| plane.hydrate(fleet_id)).await
}

/// Writes the run's memory back under the lease's fencing token.
///
/// Called before the report: the daemon settles the lease on the report, and
/// a push after that would carry a token it no longer honours.
///
/// # Errors
/// A refusal — a stale token among them — or a retryable failure that outlasted
/// its attempts.
pub(crate) async fn capture(
    plane: &ControlPlane,
    fleet_id: &Uuid7,
    lease: &LeasePayload<'_>,
    memory: Vec<MemoryDelta<'static>>,
) -> Result<()> {
    let request = MemoryPushRequest {
        lease_id: Cow::Borrowed(&lease.lease_id),
        fencing_token: lease.fencing_token,
        memory,
    };
    retrying(|| plane.capture(fleet_id, &request)).await
}

/// Writes one lease's memory back mid-run, through the same fenced push.
#[derive(Debug)]
pub(crate) struct LeaseCheckpoint<'a> {
    plane: &'a ControlPlane,
    fleet_id: &'a Uuid7,
    lease: &'a LeasePayload<'a>,
}

impl<'a> LeaseCheckpoint<'a> {
    /// The checkpoint of `lease`, for `fleet_id`, pushing through `plane`.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        fleet_id: &'a Uuid7,
        lease: &'a LeasePayload<'a>,
    ) -> Self {
        Self {
            plane,
            fleet_id,
            lease,
        }
    }
}

#[async_trait::async_trait]
impl Checkpoint for LeaseCheckpoint<'_> {
    async fn push(&self, memory: Vec<MemoryDelta<'static>>) {
        if let Err(failure) = capture(self.plane, self.fleet_id, self.lease, memory).await {
            let error_code = failure.code().as_str();
            let lease_id = self.lease.lease_id.as_ref();
            let event = EVENT_CHECKPOINT_FAILED;
            tracing::warn!(
                error_code,
                lease_id,
                event,
                "a mid-run memory checkpoint was not written; the push before the report carries it"
            );
        }
    }
}

#[cfg(test)]
#[path = "memory/tests.rs"]
mod tests;
