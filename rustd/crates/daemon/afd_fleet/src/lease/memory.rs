//! The runner's memory verbs: what a run is seeded with, what it learned, and
//! what it asks for past its window.
//!
//! # They authorize differently
//!
//! Hydrate asks "does this runner hold a live lease on this fleet, and has no
//! reclaim moved the fleet past it" — a read of a fleet's own memory by the
//! runner currently running it.
//!
//! Capture and recall ask more. The body names the lease, exactly as a report
//! does; the statement cross-checks that lease against the path's fleet, so a
//! runner cannot reach one fleet's memory holding another's lease; and the
//! token is fenced by the rule report and renew apply — it must be the lease's
//! own, and the lease must still hold the fleet — so a holder a reclaim has
//! already superseded reads and writes nothing, whatever token it presents.
//!
//! The fence statements in [`crate::lease::fence`] scope the lease to this
//! runner and fleet; `Fence::current` and `Fence::holds` then apply the
//! supersession and own-token rule report and renew share. Past the fence,
//! memory is `afd_memory`'s: the grants, the store and the window are decided
//! there.
//!
//! # Capture authority spans its write
//!
//! Reads use a live fence observation. Capture holds the fleet grant row and
//! the lease on the write transaction, then rechecks the lease after writing.
//! A reclaim waits for that transaction, and a delayed superseded capture
//! cannot replace the new holder's entry.

use af_core::id::Uuid7;
use af_observability::verb_span::{CaptureStage, HydrateStage, VerbClock};
use af_wire::memory::{
    MemoryDelta, MemoryHydrateResponse, MemoryPushRequest, MemoryRecallRequest,
    MemoryRecallResponse,
};
use afd_db::Precondition as _;
use afd_memory::Captured;
use jiff::Timestamp;
use sqlx::Acquire as _;

use crate::error::{Result, lease_not_found, stale_fence};
use crate::lease::fence::Fence;
use crate::lease::pull::LeaseService;

/// The lease a memory write is fenced on, by its four names.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Holder<'a> {
    pub(crate) runner_id: &'a Uuid7,
    pub(crate) fleet_id: &'a Uuid7,
    pub(crate) lease_id: &'a Uuid7,
    pub(crate) fencing_token: u64,
}

/// A run's memory was persisted.
const EVENT_CAPTURED: &str = "memory_captured";

/// A memory verb was fenced out — a superseded lease, or a token not the
/// lease's own — and nothing was read or written.
const EVENT_FENCED: &str = "memory_verb_fenced";

impl LeaseService {
    /// The memory window that seeds one run.
    ///
    /// # Errors
    /// Refuses a runner holding no live lease on `fleet_id`, and one whose lease
    /// the fleet has moved past. Reports a memory store that would not answer.
    pub async fn hydrate(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
    ) -> Result<MemoryHydrateResponse<'static>> {
        let mut clock = VerbClock::<HydrateStage>::start();
        let fenced = self.leases.live_fence_for_fleet(runner_id, fleet_id).await;
        clock.mark(HydrateStage::Fence);
        let Some(fence) = fenced? else {
            return Err(lease_not_found());
        };
        if !fence.current() {
            return Err(superseded(fleet_id, None, fence));
        }
        // Named only once the fence proves the lease is this runner's.
        clock.agent(fleet_id.as_str());
        let window = self.memories.hydrate(fleet_id).await;
        clock.mark(HydrateStage::MemoryRead);
        Ok(window?)
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
        now: Timestamp,
    ) -> Result<Captured> {
        let lease = crate::error::wire_lease(&request.lease_id)?;
        let holder = Holder {
            runner_id,
            fleet_id,
            lease_id: &lease,
            fencing_token: request.fencing_token,
        };
        let mut clock = VerbClock::<CaptureStage>::start();
        self.capture_for(holder, &request.memory, now, &mut clock)
            .await
    }

    /// [`LeaseService::capture`]'s fence and write, for a caller holding the parts
    /// rather than a push body: the report verb, whose body carries the run's
    /// memory beside its result. Marks the fence and the write on `clock`.
    pub(crate) async fn capture_for(
        &self,
        holder: Holder<'_>,
        memory: &[MemoryDelta<'_>],
        now: Timestamp,
        clock: &mut VerbClock<CaptureStage>,
    ) -> Result<Captured> {
        let Holder {
            runner_id,
            fleet_id,
            lease_id,
            fencing_token,
        } = holder;
        let fenced = self
            .fenced(runner_id, fleet_id, lease_id, fencing_token)
            .await;
        clock.mark(CaptureStage::Fence);
        fenced?;
        clock.agent(fleet_id.as_str());
        let written = self.capture_locked(holder, memory, now).await;
        clock.mark(CaptureStage::MemoryWrite);
        if written.is_err() {
            af_observability::producers::memory::push_failed();
        }
        let counted = written?;

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
        {
            let (fleet_id, event, message) = (
                &fleet,
                &EVENT_CAPTURED,
                format_args!("a run's memory was persisted"),
            );
            tracing::debug!(
                message,
                fleet_id,
                stored,
                skipped,
                truncated,
                unpublished,
                swept,
                evicted,
                event
            );
        };
        Ok(counted)
    }

    async fn capture_locked(
        &self,
        holder: Holder<'_>,
        memory: &[MemoryDelta<'_>],
        now: Timestamp,
    ) -> Result<Captured> {
        let mut connection = self.leases.pool().acquire().await?;
        let mut tx = connection
            .begin()
            .await
            .map_err(crate::error::query("begin memory capture"))?;
        sqlx::query(include_str!("fence/lock_memory_fleet.sql"))
            .bind(holder.fleet_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(crate::error::query("hold memory fleet"))?
            .ok_or_else(lease_not_found)?;
        let fence = super::write_fence::WriteFence::new(
            holder.runner_id.clone(),
            holder.lease_id.clone(),
            holder.fencing_token,
        );
        if !fence
            .holds(&mut tx)
            .await
            .map_err(crate::error::query("hold memory lease"))?
        {
            return Err(stale_fence());
        }
        let captured = self
            .memories
            .capture_on(&mut tx, holder.fleet_id, memory, now)
            .await?;
        if !fence
            .holds(&mut tx)
            .await
            .map_err(crate::error::query("recheck memory lease"))?
        {
            return Err(stale_fence());
        }
        tx.commit()
            .await
            .map_err(crate::error::query("commit fenced memory"))?;
        captured.record();
        Ok(captured)
    }

    /// The fleet's entries holding the request's query, and — for a fleet
    /// granted to read shared memory — the workspace's shared ones.
    ///
    /// # Errors
    /// As [`LeaseService::capture`]'s fence, and a store that would not answer.
    pub async fn recall(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryRecallRequest<'_>,
    ) -> Result<MemoryRecallResponse<'static>> {
        self.fenced(
            runner_id,
            fleet_id,
            &crate::error::wire_lease(&request.lease_id)?,
            request.fencing_token,
        )
        .await?;
        Ok(self
            .memories
            .recall(fleet_id, &request.query, request.limit)
            .await?)
    }

    /// Proves `lease_id` is this runner's live lease on `fleet_id`, that the
    /// fleet has not moved past it, and that `token` is its own.
    async fn fenced(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        lease_id: &Uuid7,
        token: u64,
    ) -> Result<()> {
        let Some(fence) = self
            .leases
            .live_fence_for_lease(runner_id, lease_id, fleet_id)
            .await?
        else {
            return Err(lease_not_found());
        };
        if !fence.holds(token) {
            return Err(superseded(fleet_id, Some(token), fence));
        }
        Ok(())
    }
}

/// The refusal a fenced memory verb answers, logged once for every caller.
///
/// `token` is what the request presented; a hydrate presents none. `own`
/// beside it tells a superseded lease (`own` below `live_seq`) from a token
/// that is not the lease's.
fn superseded(fleet_id: &Uuid7, token: Option<u64>, fence: Fence) -> crate::Error {
    let fleet = fleet_id.as_str();
    let own = fence.own();
    let live_seq = fence.live_seq();
    {
        let (fleet_id, fencing_token, event, message) = (
            &fleet,
            &token,
            &EVENT_FENCED,
            format_args!(
                "a superseded lease, or a token not its own, reached memory; nothing was read or stored"
            ),
        );
        tracing::debug!(message, fleet_id, fencing_token, own, live_seq, event);
    };
    stale_fence()
}
