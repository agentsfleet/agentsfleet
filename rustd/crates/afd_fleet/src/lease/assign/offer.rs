//! From the ready fleets a poll found to one won claim.
//!
//! Two passes end here: the fleets whose sandboxes the runner holds, read
//! first, then the partition's. Both reach Postgres through the same candidate
//! scan and the same claim, so a fleet found either way is held to the same
//! eligibility.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::Ready;

use super::diagnostics::{EVENT_READY_PEEK_FAILED, warn_queue};
use super::{PollCost, peeked};
use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

impl Leases {
    /// The work of a fleet whose sandbox the runner holds, when one has any.
    ///
    /// Each fleet's readiness is read on its own, so a held fleet is found on
    /// the holder's next poll whatever partition its mark sits in. Only a
    /// fleet with a mark reaches Postgres, through the same candidate scan the
    /// partition's fleets take, so a runner naming a fleet gains nothing the
    /// scan would refuse it, and an idle poll still costs no Postgres at all.
    pub(super) async fn held_first(
        &self,
        runner_id: &Uuid7,
        held: &[Uuid7],
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        // One read per held fleet, and a runner holds at most one per worker.
        let mut ready = Vec::new();
        for fleet in held {
            let token = self
                .ready()
                .token_for(fleet.as_str())
                .await
                .inspect_err(|error| warn_queue(EVENT_READY_PEEK_FAILED, runner_id, error))?;
            if let Some(token) = token {
                let fleet_id = fleet.as_str().to_owned();
                ready.push(Ready { fleet_id, token });
            }
        }
        if ready.is_empty() {
            return Ok(None);
        }
        self.first_offered(runner_id, &ready, now, cost).await
    }

    /// The first of `ready` the candidate scan offers and this runner wins,
    /// tried in the scan's sticky order: one statement for the scan, then one
    /// per claim.
    pub(super) async fn first_offered(
        &self,
        runner_id: &Uuid7,
        ready: &[Ready],
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let ids: Vec<&str> = ready.iter().map(|entry| entry.fleet_id.as_str()).collect();
        cost.database_roundtrips += 1;
        let offered = self.candidates(runner_id, &ids, now).await?;
        for (fleet_id, token) in peeked(offered, ready) {
            cost.database_roundtrips += 1;
            if let Some(acquired) = self.try_candidate(&fleet_id, token, runner_id, now).await? {
                return Ok(Some(acquired));
            }
        }
        Ok(None)
    }
}
