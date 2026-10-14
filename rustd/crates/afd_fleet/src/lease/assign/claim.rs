//! Claiming one candidate and taking its work: a lapsed holder's event, else
//! the fleet's oldest pending entry, else a new one.
//!
//! Every Dragonfly command this sends joins the poll's [`PollCost`]: the
//! stream reads, the mark it clears on a drained fleet, and the
//! acknowledgement of an entry no reader can decode. The tally is what
//! `agentsfleet_lease_poll_dragonfly_commands_total` publishes, so a command
//! left out of it is load the metric hides.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::LEASE_TTL_MS;
use afd_dragonfly::ReadyToken;
use afd_observability::producers;

use super::diagnostics::{EVENT_LEASE_RECLAIMED, drop_undecodable};
use super::{PollCost, runner_consumer, started};
use crate::error::Result;
use crate::lease::affinity;
use crate::lease::envelope::{Acquired, from_fresh, from_reclaim};
use crate::lease::store::Leases;

impl Leases {
    /// Claim one candidate and take its work, or answer `None` and move on.
    pub(super) async fn try_candidate(
        &self,
        fleet_id: &Uuid7,
        token: &ReadyToken,
        runner_id: &Uuid7,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        // Recorded on the ONE exit that hands work out. A `None` — a lost
        // claim, an empty stream, a dropped entry — reaches nothing here,
        // because nothing started.
        self.try_candidate_unrecorded(fleet_id, token, runner_id, now, cost)
            .await
            .inspect(|found| {
                if let Some(acquired) = found {
                    producers::fleet::run_started(started(acquired.kind));
                    affinity::count_resumed(acquired);
                }
            })
    }

    /// [`Self::try_candidate`] without the recording, so the outcome exists
    /// before anything is said about it.
    async fn try_candidate_unrecorded(
        &self,
        fleet_id: &Uuid7,
        token: &ReadyToken,
        runner_id: &Uuid7,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let Some(claimed) = self.claim(fleet_id, runner_id, now, LEASE_TTL_MS).await? else {
            // Taken by a live holder. No event was read, so nothing is orphaned.
            return Ok(None);
        };
        // A fault between the win and a lease frees the claim before it is
        // raised, through the release the pull's endings share. Left held, the
        // held-slot filter would hide the fleet for a full TTL.
        match self
            .take_claimed(fleet_id, &claimed, token, runner_id, now, cost)
            .await
        {
            Err(fault) => {
                self.let_go(fleet_id, claimed.fence, now).await;
                Err(fault)
            }
            outcome => outcome,
        }
    }

    /// The work a won claim takes: a lapsed holder's event, else a fresh one.
    async fn take_claimed(
        &self,
        fleet_id: &Uuid7,
        claimed: &crate::lease::affinity::Claimed,
        token: &ReadyToken,
        runner_id: &Uuid7,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        // A won claim over a lapsed holder means its lease is still `active`
        // and still names the work it never finished.
        if let Some(prior) = self.reclaim_prior_active(fleet_id, now).await? {
            let fleet = fleet_id.as_str();
            let runner = runner_id.as_str();
            let lease = prior.lease_id.as_str();
            let event_id = prior.event_id.as_str();
            let fence = claimed.fence.as_i64();
            tracing::debug!(
                event = EVENT_LEASE_RECLAIMED,
                fleet_id = fleet,
                runner_id = runner,
                lease_id = lease,
                agentsfleet_event_id = event_id,
                fencing_token = fence,
                "re-leasing a lapsed holder's event under a higher fence"
            );
            return from_reclaim(fleet_id, claimed, token, prior).map(Some);
        }
        self.acquire_fresh(fleet_id, claimed, token, now, cost)
            .await
    }

    /// Pull the next event for a claimed fleet: the group's oldest pending
    /// entry first, whichever consumer holds it, then a new entry.
    ///
    /// Pending-first is safe precisely BECAUSE the claim was won: that proves no
    /// live lease exists, so a pending entry is a re-poll, a parked event or a
    /// dead replica's strand rather than work somebody else is doing.
    async fn acquire_fresh(
        &self,
        fleet_id: &Uuid7,
        claimed: &crate::lease::affinity::Claimed,
        token: &ReadyToken,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let streams = self.streams();
        let fleet = fleet_id.as_str();
        let Some(pulled) = self
            .read_fresh(fleet, &runner_consumer(), &mut cost.dragonfly_commands)
            .await?
        else {
            // Both reads answered, and both were empty — the only evidence this
            // code ever has that a fleet holds nothing deliverable. Free the
            // claim so the next event is not blocked behind it, then clear the
            // mark this poll peeked: a drained fleet left marked is a claim,
            // a read and a release on every poll that samples it, forever.
            self.release(fleet_id, claimed.fence, now).await?;
            producers::fleet::lease_claimed_empty();
            cost.dragonfly_commands += 1;
            self.clear_mark(fleet_id, token).await;
            return Ok(None);
        };
        let event = pulled.event;
        match from_fresh(fleet_id, claimed, token, &event, pulled.redelivered) {
            Ok(acquired) => Ok(Some(acquired)),
            Err(undecodable) => {
                cost.dragonfly_commands += 1;
                drop_undecodable(&streams, fleet, &event.receipt, &undecodable).await;
                // Freed for the reason the empty arm frees it: this fleet holds
                // nothing this poll can lease. Holding the claim would cost a
                // full TTL of silence on a fleet whose next event may be fine.
                self.release(fleet_id, claimed.fence, now).await?;
                Ok(None)
            }
        }
    }
}
