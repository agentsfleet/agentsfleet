//! Choosing the next fleet and event for a polling runner.
//!
//! One pass per `lease` call — no server-side long-poll loop; the runner
//! re-polls on the backoff the reply carries. The pass is READY-FIRST, and the
//! ordering is the design:
//!
//! 1. Peek the shared readiness index, BEFORE touching Postgres. An empty index
//!    answers no-work with zero database round-trips, which is the dominant
//!    steady state on any deployment holding more fleets than concurrent
//!    events.
//! 2. Run the candidate query, restricted to those fleets and capped. Readiness
//!    NARROWS the input; it never decides eligibility — the label gate and the
//!    sticky ordering are properties of the query.
//! 3. Per candidate: claim it. A loser moves on having read no event, because
//!    the claim precedes the read.
//! 4. Won with a prior active lease → RECLAIM that dead holder's event. Won with
//!    none → FRESH: the consumer's own pending list first, then a new entry.
//!
//! Every non-success exit after a win frees the claim, so an abandoned claim
//! costs one poll rather than a full TTL of silence on that fleet.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::LEASE_TTL_MS;
use afd_observability::metrics::label::fleet::RunStart;
use afd_observability::producers;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::lease::envelope::{Acquired, Kind, from_fresh, from_reclaim};
use crate::lease::sql;
use crate::lease::store::Leases;

mod diagnostics;

pub(crate) use diagnostics::warn_queue_fleet;
use diagnostics::{EVENT_LEASE_RECLAIMED, EVENT_READY_PEEK_FAILED, drop_undecodable, warn_queue};

/// Statement name, for the context a query failure carries.
const CONTEXT_CANDIDATES: &str = "lease candidate scan";

/// The `core.fleets.status` value a leasable fleet carries.
pub(crate) const FLEET_STATUS_ACTIVE: &str = "active";

/// How many ready fleets one poll will consider.
///
/// `constants.zig`'s `MAX_READY_CANDIDATES_PER_POLL`. The ceiling is what makes
/// per-poll cost independent of how many fleets exist — without it a runner
/// polling an idle deployment pays for every fleet on it, every second.
pub const MAX_READY_CANDIDATES_PER_POLL: usize = 64;

/// What one lease poll cost, gathered as it runs.
///
/// The ratio is what an operator reads: candidates per poll says how much a
/// poll examined, and round-trips per poll says how much of that reached
/// Postgres. Either number alone is unreadable, which is why they are tallied
/// together and recorded together.
#[derive(Debug, Default)]
struct PollCost {
    /// Fleets the readiness index offered this poll.
    candidates_scanned: u64,
    /// Statements this poll issued.
    database_roundtrips: u64,
}

impl Leases {
    /// Select the next work for `runner_id`, or `None` when nothing is leasable
    /// this pass.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. "Nothing to do" is
    /// `Ok(None)`, not an error — the runner backs off and re-polls.
    pub async fn select(&self, runner_id: &Uuid7, now: UnixMillis) -> Result<Option<Acquired>> {
        self.select_recording(runner_id, now).await.0
    }

    /// The poll, its outcome, and what it cost.
    ///
    /// The one body the public entry point above and the `test-util`
    /// measurement in `assign/measured.rs` both run, so a suite asserting on
    /// the cost is asserting on the tally production publishes rather than on
    /// a second one written beside it.
    async fn select_recording(
        &self,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> (Result<Option<Acquired>>, PollCost) {
        let mut cost = PollCost::default();
        let selected = self.select_counted(runner_id, now, &mut cost).await;
        // On EVERY exit path, including the one where the peek itself failed:
        // a poll that could not read the index is still a poll, and a total
        // that skipped it would make idle cost look lower than it is.
        producers::fleet::lease_polled(cost.candidates_scanned, cost.database_roundtrips);
        (selected, cost)
    }

    /// [`Leases::select`] without the recording, tallying what it cost.
    async fn select_counted(
        &self,
        runner_id: &Uuid7,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        // One partition per poll, the next in the rotation: the read stays one
        // bounded round trip, and the partitions this poll did not visit are
        // the next polls' — whichever runner makes them.
        let partition = self.cursor().advance();
        let ready = self
            .ready()
            .peek(partition, MAX_READY_CANDIDATES_PER_POLL)
            .await
            .inspect_err(|error| warn_queue(EVENT_READY_PEEK_FAILED, runner_id, error))?;
        cost.candidates_scanned = u64::try_from(ready.len()).unwrap_or(u64::MAX);
        // The readiness depth this poll saw, published for the gauge that
        // reports it: the index is a network round trip, and a collection
        // callback cannot make one.
        producers::fleet::ready_depth_observed(cost.candidates_scanned);
        // The zero-Postgres path. Returning here is what makes idle cost scale
        // with runner count alone instead of runners × fleets.
        if ready.is_empty() {
            return Ok(None);
        }

        let ids: Vec<&str> = ready.iter().map(|entry| entry.fleet_id.as_str()).collect();
        cost.database_roundtrips += 1;
        for fleet_id in self.candidates(runner_id, &ids).await? {
            cost.database_roundtrips += 1;
            if let Some(acquired) = self.try_candidate(&fleet_id, runner_id, now).await? {
                return Ok(Some(acquired));
            }
        }
        Ok(None)
    }

    /// The eligible fleets among `ready`, in the query's own sticky order.
    ///
    /// The ordering must come from the statement and not from the peek, because
    /// sticky preference lives in its `ORDER BY`.
    async fn candidates(&self, runner_id: &Uuid7, ready: &[&str]) -> Result<Vec<Uuid7>> {
        let mut connection = self.pool().acquire().await?;
        let rows = sqlx::query(sql::lease::SELECT_READY_CANDIDATES)
            .bind(FLEET_STATUS_ACTIVE)
            .bind(runner_id.as_str())
            .bind(ready)
            .bind(i64::try_from(MAX_READY_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_CANDIDATES))?;

        rows.iter()
            .map(|row| {
                let id: String = row.try_get(0).map_err(query(CONTEXT_CANDIDATES))?;
                Uuid7::parse(&id).map_err(crate::error::row_malformed("core.fleets", "id"))
            })
            .collect()
    }

    /// Claim one candidate and take its work, or answer `None` and move on.
    async fn try_candidate(
        &self,
        fleet_id: &Uuid7,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Acquired>> {
        // Recorded on the ONE exit that hands work out. A `None` — a lost
        // claim, an empty stream, a dropped entry — reaches nothing here,
        // because nothing started.
        self.try_candidate_unrecorded(fleet_id, runner_id, now)
            .await
            .inspect(|found| {
                if let Some(acquired) = found {
                    producers::fleet::run_started(started(acquired.kind));
                }
            })
    }

    /// [`Self::try_candidate`] without the recording, so the outcome exists
    /// before anything is said about it.
    async fn try_candidate_unrecorded(
        &self,
        fleet_id: &Uuid7,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Acquired>> {
        let Some(claimed) = self.claim(fleet_id, runner_id, now, LEASE_TTL_MS).await? else {
            // Taken by a live holder. No event was read, so nothing is orphaned.
            return Ok(None);
        };

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
            return from_reclaim(fleet_id, &claimed, prior).map(Some);
        }
        self.acquire_fresh(fleet_id, &claimed, now).await
    }

    /// Pull the next event for a claimed fleet: this consumer's own pending
    /// list first, then a new entry.
    ///
    /// Pending-first is safe precisely BECAUSE the claim was won: that proves no
    /// live lease exists, so a pending entry is a re-poll or a recovered strand
    /// rather than work somebody else is doing.
    async fn acquire_fresh(
        &self,
        fleet_id: &Uuid7,
        claimed: &crate::lease::affinity::Claimed,
        now: UnixMillis,
    ) -> Result<Option<Acquired>> {
        let streams = self.streams();
        let fleet = fleet_id.as_str();
        let Some(event) = self.read_fresh(fleet, &runner_consumer()).await? else {
            // Both reads answered, and both were empty — the only evidence this
            // code ever has that a fleet holds nothing deliverable. Free the
            // claim so the next event is not blocked behind it.
            self.release(fleet_id, claimed.fence, now).await?;
            return Ok(None);
        };
        match from_fresh(fleet_id, claimed, &event) {
            Ok(acquired) => Ok(Some(acquired)),
            Err(undecodable) => {
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

/// The label a granted lease's kind is counted under.
///
/// A total mapping: every kind a grant can carry has a start label, so a
/// grant cannot go uncounted — and a kind added without a label is a
/// compile error here rather than a series that never appears.
pub(crate) const fn started(kind: Kind) -> RunStart {
    match kind {
        Kind::Fresh => RunStart::Fresh,
        Kind::Reclaim => RunStart::Reclaimed,
    }
}

/// The stable consumer name this daemon reads under.
///
/// One name per process rather than per request: the pending list belongs to
/// the CONSUMER, so a name that changed per poll would strand every entry the
/// previous name had claimed.
///
/// `pub` because the reclaim sweeper claims stranded entries INTO this name and
/// the lease path reads OUT of it, and the two must be the same string. A
/// sweeper claiming into a name nothing reads would re-strand exactly the
/// entries it exists to rescue — the failure would look like the sweeper
/// working perfectly, since it would report claims every pass.
#[must_use]
pub fn runner_consumer() -> String {
    format!("agentsfleetd-{}", std::process::id())
}

#[cfg(feature = "test-util")]
pub mod measured;

#[cfg(all(test, feature = "test-util"))]
mod tests;
