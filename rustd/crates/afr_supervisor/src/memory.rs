//! A fleet's durable memory: read at lease start, written back fenced.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::{MemoryDelta, MemoryPushRequest};

use crate::client::{Body, ControlPlane, retrying};
use crate::error::Result;

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

#[cfg(test)]
#[path = "memory/tests.rs"]
mod tests;
