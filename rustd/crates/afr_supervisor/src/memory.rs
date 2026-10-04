//! A fleet's durable memory: read at lease start, searched past the window
//! when a recall falls short, and written back fenced.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::{
    MemoryDelta, MemoryPushRequest, MemoryRecallRequest, MemoryRecallResponse, SharedMemory,
};
use afr_agent::Checkpoint;

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
    let request = push_request(lease, memory);
    retrying(|| plane.capture(fleet_id, &request)).await
}

/// `memory`, fenced by `lease`'s token.
fn push_request<'a>(
    lease: &'a LeasePayload<'_>,
    memory: Vec<MemoryDelta<'static>>,
) -> MemoryPushRequest<'a> {
    MemoryPushRequest {
        lease_id: Cow::Borrowed(&lease.lease_id),
        fencing_token: lease.fencing_token,
        memory,
    }
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
    /// One attempt, no retry: the push before the report retries, and carries
    /// every entry again.
    async fn push(&self, memory: Vec<MemoryDelta<'static>>) -> afr_agent::Result<()> {
        let request = push_request(self.lease, memory);
        (self.plane.capture(self.fleet_id, &request).await).map_err(afr_agent::Error::checkpoint)
    }
}

/// Asks the daemon for memory a run's window missed, fenced by the lease's
/// token. Never retried: a recall that gets no answer answers from the
/// window, and a retry would only make the model wait longer for the same.
#[derive(Debug)]
pub(crate) struct Recaller<'a> {
    plane: &'a ControlPlane,
    fleet_id: &'a Uuid7,
    lease: &'a LeasePayload<'a>,
}

impl<'a> Recaller<'a> {
    /// Recalls for `fleet_id` under `lease`.
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
impl afr_memory::Recall for Recaller<'_> {
    async fn recall(
        &self,
        query: &str,
        limit: usize,
    ) -> afr_memory::Result<MemoryRecallResponse<'static>> {
        let request = MemoryRecallRequest {
            lease_id: Cow::Borrowed(&self.lease.lease_id),
            fencing_token: self.lease.fencing_token,
            query: Cow::Borrowed(query),
            limit,
        };
        let unanswered =
            |failure: crate::error::Error| afr_memory::Error::unanswered(failure.code());
        let body = self
            .plane
            .recall(self.fleet_id, &request)
            .await
            .map_err(unanswered)?;
        let found = body
            .decode::<MemoryRecallResponse<'_>>()
            .map_err(unanswered)?;
        Ok(MemoryRecallResponse {
            memory: found
                .memory
                .into_iter()
                .map(MemoryDelta::into_owned)
                .collect(),
            shared: found
                .shared
                .into_iter()
                .map(SharedMemory::into_owned)
                .collect(),
        })
    }
}

#[cfg(test)]
#[path = "memory/tests.rs"]
mod tests;
