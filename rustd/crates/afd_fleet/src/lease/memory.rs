//! The two memory verbs: what a run is seeded with, and what it learned.
//!
//! # They authorize differently, and both are the fleet's own `WHERE`
//!
//! Hydrate asks only "does this runner hold a live lease on this fleet" — a
//! read of a fleet's own memory by the runner currently running it.
//!
//! Capture asks more, because it WRITES. The body names the lease, exactly as a
//! report does; the statement cross-checks that lease against the path's fleet,
//! so a runner cannot reach one fleet's memory holding another's lease; and the
//! token is fenced, so a holder a reclaim has superseded writes nothing.
//!
//! Neither check is performed here. Both are the `WHERE` of
//! [`SELECT_LIVE_FENCE_BY_FLEET`](crate::sql::memory::SELECT_LIVE_FENCE_BY_FLEET)
//! and its sibling — a scope the database enforces cannot be a comparison this
//! code forgets to make.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::{HYDRATE_WINDOW_BYTES, MemoryDelta, MemoryPushRequest};

use crate::error::{Result, lease_not_found, stale_fence};
use crate::lease::pull::Plane;
use crate::memory::Captured;
use crate::memory::window::{self, Window};

/// A run was seeded with a memory window.
const EVENT_HYDRATED: &str = "memory_hydrated";

/// A run's memory was persisted.
const EVENT_CAPTURED: &str = "memory_captured";

impl Plane {
    /// The memory window that seeds one run.
    ///
    /// # Errors
    /// Refuses a runner holding no live lease on `fleet_id`, and reports a
    /// datastore that would not answer.
    pub async fn hydrate(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Vec<MemoryDelta<'static>>> {
        if self
            .leases
            .live_fence_for_fleet(runner_id, fleet_id, now)
            .await?
            .is_none()
        {
            return Err(lease_not_found());
        }

        let stored = self.memories.list(fleet_id).await?;
        let held = stored.len();
        let Window { kept, dropped } = window::select(stored, HYDRATE_WINDOW_BYTES);

        // Hoisted for the `log` bridge's duplicated field expressions.
        let fleet = fleet_id.as_str();
        let hydrated = kept.len();
        let dropped_count = dropped.len();
        let dropped_bytes = window::total_bytes(&dropped);
        tracing::debug!(
            fleet_id = fleet,
            held,
            hydrated,
            dropped = dropped_count,
            dropped_bytes,
            event = EVENT_HYDRATED,
            "a run was seeded with its memory window; the rest stays durable"
        );
        Ok(kept)
    }

    /// Persist what one run learned.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not this fleet's, and a
    /// holder the fleet has superseded. Reports a datastore that would not
    /// answer. A delta refused for its shape is COUNTED, not an error — one
    /// malformed entry must not lose a run's whole memory.
    pub async fn capture(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryPushRequest<'_>,
        now: UnixMillis,
    ) -> Result<Captured> {
        let Some(live) = self
            .leases
            .live_fence_for_lease(runner_id, &request.lease_id, fleet_id, now)
            .await?
        else {
            return Err(lease_not_found());
        };
        if request.fencing_token < live {
            let fleet = fleet_id.as_str();
            let presented = request.fencing_token;
            tracing::debug!(
                fleet_id = fleet,
                fencing_token = presented,
                live_seq = live,
                event = "memory_push_fenced",
                "a superseded holder tried to write memory; nothing was stored"
            );
            return Err(stale_fence());
        }

        let counted = self
            .memories
            .capture(fleet_id, &request.memory, now)
            .await?;

        // The CONTENT is never logged — only the tallies and the scope. A
        // memory entry is whatever a fleet learned about its user's work, and a
        // log line is the one place it must not end up.
        let fleet = fleet_id.as_str();
        let Captured {
            stored,
            skipped,
            truncated,
            swept,
            evicted,
        } = counted;
        tracing::debug!(
            fleet_id = fleet,
            stored,
            skipped,
            truncated,
            swept,
            evicted,
            event = EVENT_CAPTURED,
            "a run's memory was persisted"
        );
        Ok(counted)
    }
}
