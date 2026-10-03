//! The runner's memory verbs: what a run is seeded with, what it learned, and
//! what it asks for past its window.
//!
//! # They authorize differently, and every check is the fleet's own `WHERE`
//!
//! Hydrate asks only "does this runner hold a live lease on this fleet" — a
//! read of a fleet's own memory by the runner currently running it.
//!
//! Capture and recall ask more. The body names the lease, exactly as a report
//! does; the statement cross-checks that lease against the path's fleet, so a
//! runner cannot reach one fleet's memory holding another's lease; and the
//! token is fenced, so a holder a reclaim has superseded reads and writes
//! nothing. Both checks are the `WHERE` of the fence statements in
//! [`crate::lease::fence`]. Past the fence, memory is `afd_memory`'s: the
//! grants, the store and the window are decided there.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_memory::Captured;
use afd_wire::memory::{
    MemoryHydrateResponse, MemoryPushRequest, MemoryRecallRequest, MemoryRecallResponse,
};

use crate::error::{Result, lease_not_found, stale_fence};
use crate::lease::pull::Plane;

/// A run's memory was persisted.
const EVENT_CAPTURED: &str = "memory_captured";

/// A superseded holder reached memory, and nothing was read or written.
const EVENT_FENCED: &str = "memory_push_fenced";

impl Plane {
    /// The memory window that seeds one run.
    ///
    /// # Errors
    /// Refuses a runner holding no live lease on `fleet_id`, and reports a
    /// memory store that would not answer.
    pub async fn hydrate(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<MemoryHydrateResponse<'static>> {
        if self
            .leases
            .live_fence_for_fleet(runner_id, fleet_id, now)
            .await?
            .is_none()
        {
            return Err(lease_not_found());
        }
        Ok(self.memories.hydrate(fleet_id).await?)
    }

    /// Persist what one run learned.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not this fleet's, and a
    /// holder the fleet has superseded. Reports a store that would not answer.
    /// A delta refused for its shape, or a share from a fleet that may not
    /// publish, is COUNTED, not an error — one such entry must not lose a
    /// run's whole memory.
    pub async fn capture(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryPushRequest<'_>,
        now: UnixMillis,
    ) -> Result<Captured> {
        self.fenced(
            runner_id,
            fleet_id,
            &request.lease_id,
            request.fencing_token,
            now,
        )
        .await?;
        let counted = self
            .memories
            .capture(fleet_id, &request.memory, now)
            .await?;

        // The CONTENT is never logged — only the tallies and the scope.
        let fleet = fleet_id.as_str();
        let Captured {
            stored,
            skipped,
            truncated,
            unpublished,
            swept,
            evicted,
        } = counted;
        tracing::debug!(
            fleet_id = fleet,
            stored,
            skipped,
            truncated,
            unpublished,
            swept,
            evicted,
            event = EVENT_CAPTURED,
            "a run's memory was persisted"
        );
        Ok(counted)
    }

    /// The fleet's entries holding the request's query, and — for a fleet
    /// granted to read shared memory — the workspace's shared ones.
    ///
    /// # Errors
    /// As [`Plane::capture`]'s fence, and a store that would not answer.
    pub async fn recall(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryRecallRequest<'_>,
        now: UnixMillis,
    ) -> Result<MemoryRecallResponse<'static>> {
        self.fenced(
            runner_id,
            fleet_id,
            &request.lease_id,
            request.fencing_token,
            now,
        )
        .await?;
        Ok(self
            .memories
            .recall(fleet_id, &request.query, request.limit)
            .await?)
    }

    /// Proves `lease_id` is this runner's live lease on `fleet_id`, and that
    /// `token` is not below the fleet's live sequence.
    async fn fenced(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        lease_id: &str,
        token: u64,
        now: UnixMillis,
    ) -> Result<()> {
        let Some(live) = self
            .leases
            .live_fence_for_lease(runner_id, lease_id, fleet_id, now)
            .await?
        else {
            return Err(lease_not_found());
        };
        if token < live {
            let fleet = fleet_id.as_str();
            tracing::debug!(
                fleet_id = fleet,
                fencing_token = token,
                live_seq = live,
                event = EVENT_FENCED,
                "a superseded holder reached memory; nothing was read or stored"
            );
            return Err(stale_fence());
        }
        Ok(())
    }
}
