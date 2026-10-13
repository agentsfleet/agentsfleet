//! From the ready fleets a poll found to one won claim.
//!
//! Two passes end here: the fleets whose sandboxes the runner holds, read
//! first, then the partition's. Both reach Postgres through the same candidate
//! scan and the same claim, so a fleet found either way is held to the same
//! eligibility.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::Ready;
use afd_observability::producers;
use futures_util::future::join_all;

use super::diagnostics::warn_held_read;
use super::hint::Offer;
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
        let ready = self.held_ready(runner_id, held, cost).await;
        if ready.is_empty() {
            return Ok(None);
        }
        self.first_offered(runner_id, &ready, ready.len(), now, cost)
            .await
    }

    /// The held fleets with a readiness mark, each named once and all read at
    /// once. A read that fails costs its own fleet and no other: it is logged
    /// and skipped, and the poll goes on to the partition, because a held
    /// fleet only reorders what the partition pass would offer anyway.
    async fn held_ready(
        &self,
        runner_id: &Uuid7,
        held: &[Uuid7],
        cost: &mut PollCost,
    ) -> Vec<Ready> {
        let mut fleets: Vec<&Uuid7> = held.iter().collect();
        fleets.sort_unstable();
        fleets.dedup();
        cost.dragonfly_commands += u64::try_from(fleets.len()).unwrap_or(u64::MAX);
        let index = self.ready();
        let reads = fleets.into_iter().map(|fleet| {
            let index = &index;
            async move { (fleet, index.token_for(fleet.as_str()).await) }
        });
        join_all(reads)
            .await
            .into_iter()
            .filter_map(|(fleet, read)| match read {
                Ok(token) => token.map(|token| Ready {
                    fleet_id: fleet.as_str().to_owned(),
                    token,
                }),
                Err(error) => {
                    warn_held_read(runner_id, fleet, &error);
                    None
                }
            })
            .collect()
    }

    /// The fleet this poll leases from `offer`, if any.
    ///
    /// # Errors
    /// The offer's own error, when no peek answered, and whatever the scan or
    /// a claim raises.
    pub(super) async fn offered(
        &self,
        runner_id: &Uuid7,
        offer: Result<Offer>,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let offer = offer?;
        cost.candidates_scanned = u64::try_from(offer.ready.len()).unwrap_or(u64::MAX);
        // The readiness depth this poll saw, published for the gauge that
        // reports it: the index is a network round trip, and a collection
        // callback cannot make one.
        producers::fleet::ready_depth_observed(cost.candidates_scanned);
        // The zero-Postgres path. Skipping the scan here is what makes idle
        // cost scale with runner count alone instead of runners × fleets.
        if offer.ready.is_empty() {
            return Ok(None);
        }
        self.first_offered(runner_id, &offer.ready, offer.cursor, now, cost)
            .await
    }

    /// The first of `ready` the candidate scan offers and this runner wins:
    /// the first `cursor` of `ready` before the rest, sticky first within
    /// each. One statement for the scan, then one per claim.
    pub(super) async fn first_offered(
        &self,
        runner_id: &Uuid7,
        ready: &[Ready],
        cursor: usize,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let ids: Vec<&str> = ready.iter().map(|entry| entry.fleet_id.as_str()).collect();
        let first: Vec<&str> = ids.iter().take(cursor).copied().collect();
        cost.database_roundtrips += 1;
        let offered = self.candidates(runner_id, &ids, &first, now).await?;
        for (fleet_id, token) in peeked(offered, ready) {
            cost.database_roundtrips += 1;
            if let Some(acquired) = self
                .try_candidate(&fleet_id, token, runner_id, now, cost)
                .await?
            {
                return Ok(Some(acquired));
            }
        }
        Ok(None)
    }
}
