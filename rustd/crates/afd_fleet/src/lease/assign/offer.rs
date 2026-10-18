//! From the ready fleets a poll found to one won claim.
//!
//! Two passes end here: the fleets whose sandboxes the runner holds, read
//! first, then the partition's. Both reach Postgres through the same candidate
//! scan and the same claim, so a fleet found either way is held to the same
//! eligibility.

use afd_core::id::Uuid7;
use afd_dragonfly::{Partition, Ready};
use afd_observability::producers;
use futures_util::future::join_all;

use super::diagnostics::warn_held_read;
use super::wake::Offer;
use super::{MAX_READY_CANDIDATES_PER_POLL, PollCost, peeked};
use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

/// What one pass over offered fleets ended with: the fleet it leased, if any,
/// the fleets the scan offered after it, which it never tried, and whether the
/// scan was cut at its ceiling behind that lease.
#[derive(Debug, Default)]
pub(super) struct Pass {
    pub(super) acquired: Option<Acquired>,
    pub(super) untried: Vec<Uuid7>,
    /// The scan returned its ceiling and the pass leased one of them, so
    /// eligible fleets past the ceiling, a rung partition's among them, were
    /// never offered at all.
    pub(super) cut: bool,
}

impl Pass {
    /// Whether this pass left `partition` work it could have offered the
    /// next poll: a fleet the scan offered from it that the pass never tried,
    /// or any fleet at all when the scan was cut behind a lease.
    ///
    /// A cut rings without knowing the partition had a fleet past it, so it
    /// may cost the next poll one peek for nothing; it can never loop,
    /// because only a pass that leased is cut.
    pub(super) fn left_work_in(&self, partition: Partition) -> bool {
        self.cut
            || self
                .untried
                .iter()
                .any(|fleet| Partition::of(fleet.as_str()) == partition)
    }
}

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
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let ready = self.held_ready(runner_id, held, cost).await;
        if ready.is_empty() {
            return Ok(None);
        }
        self.first_offered(runner_id, &ready, ready.len(), cost)
            .await
            .map(|claimed| claimed.acquired)
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

    /// The fleet this poll leases from `offer`, if any, and the offered
    /// fleets it never tried.
    ///
    /// # Errors
    /// The offer's own error, when no peek answered, and whatever the scan or
    /// a claim raises.
    pub(super) async fn offered(
        &self,
        runner_id: &Uuid7,
        offer: Result<Offer>,
        cost: &mut PollCost,
    ) -> Result<Pass> {
        let offer = offer?;
        cost.candidates_scanned = u64::try_from(offer.ready.len()).unwrap_or(u64::MAX);
        // The readiness depth this poll saw, published for the gauge that
        // reports it: the index is a network round trip, and a collection
        // callback cannot make one.
        producers::fleet::ready_depth_observed(cost.candidates_scanned);
        // The zero-Postgres path. Skipping the scan here is what makes idle
        // cost scale with runner count alone instead of runners × fleets.
        if offer.ready.is_empty() {
            return Ok(Pass::default());
        }
        self.first_offered(runner_id, &offer.ready, offer.from_cursor, cost)
            .await
    }

    /// The first of `ready` the candidate scan offers and this runner wins:
    /// the first `cursor` of `ready` before the rest, sticky first within
    /// each, and the offered fleets after it. One statement for the scan,
    /// then one per claim.
    pub(super) async fn first_offered(
        &self,
        runner_id: &Uuid7,
        ready: &[Ready],
        from_cursor: usize,
        cost: &mut PollCost,
    ) -> Result<Pass> {
        let ids: Vec<&str> = ready.iter().map(|entry| entry.fleet_id.as_str()).collect();
        let cursor_ids: Vec<&str> = ids.iter().take(from_cursor).copied().collect();
        cost.database_roundtrips += 1;
        let offered = self.candidates(runner_id, &ids, &cursor_ids).await?;
        let cut = offered.len() >= MAX_READY_CANDIDATES_PER_POLL;
        let mut queue = peeked(offered, ready);
        while let Some((fleet_id, token)) = queue.next() {
            cost.database_roundtrips += 1;
            if let Some(acquired) = self
                .try_candidate(&fleet_id, token, runner_id, cost)
                .await?
            {
                let untried = queue.map(|(fleet, _token)| fleet).collect();
                return Ok(Pass {
                    acquired: Some(acquired),
                    untried,
                    cut,
                });
            }
        }
        Ok(Pass::default())
    }
}
