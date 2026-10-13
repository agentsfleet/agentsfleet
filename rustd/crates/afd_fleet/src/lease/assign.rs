//! Choosing the next fleet and event for a polling runner.
//!
//! One pass per `lease` call — no server-side long-poll loop; the runner
//! re-polls on the backoff the reply carries. The pass is READY-FIRST, and the
//! ordering is the design:
//!
//! 1. Peek the shared readiness index, BEFORE touching Postgres: one of the
//!    directory's oldest dirty partitions and the rotation's next one. An empty
//!    index answers no-work with zero database round-trips, which is the
//!    dominant steady state on any deployment holding more fleets than
//!    concurrent events.
//! 2. Run the candidate query, restricted to those fleets and capped. Readiness
//!    NARROWS the input; it never decides eligibility — the label gate, the
//!    held-slot filter and the sticky ordering are properties of the query.
//! 3. Per candidate: claim it. A loser moves on having read no event, because
//!    the claim precedes the read.
//! 4. Won with a prior active lease → RECLAIM that dead holder's event. Won with
//!    none → FRESH: the group's oldest pending entry, taken over from whichever
//!    consumer holds it, then a new entry.
//! 5. Won and found nothing → the fleet is DRAINED: free the claim and clear
//!    its mark with the generation step 1 peeked, so the next poll does not
//!    pay for it. A mark written since is a newer generation and survives.
//!
//! Every non-success exit after a win frees the claim, a fault included, so an
//! abandoned claim costs one poll rather than a full TTL of silence on that
//! fleet.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::RUNNER_OFFLINE_AFTER_MS;
use afd_dragonfly::{Ready, ReadyToken};
use afd_observability::metrics::label::fleet::RunStart;
use afd_observability::producers;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::sql;
use crate::lease::store::Leases;

mod claim;
mod diagnostics;
mod hint;
mod offer;

pub(crate) use diagnostics::warn_queue_fleet;

/// Statement name, for the context a query failure carries.
const CONTEXT_CANDIDATES: &str = "lease candidate scan";

/// How many ready fleets one peek samples, and how many candidates one poll's
/// scan returns.
///
/// The ceiling is what makes per-poll cost independent of how many fleets
/// exist — without it a runner polling an idle deployment pays for every fleet
/// on it, every second. A poll peeks at most two partitions, so its scan reads
/// at most twice this many identifiers.
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
    /// Commands this poll sent Dragonfly: two on an idle poll, the directory
    /// read and the rotation's peek. A claim adds its stream reads, and the
    /// mark clear or the dead entry's acknowledgement when it takes nothing.
    dragonfly_commands: u64,
}

impl Leases {
    /// Select the next work for `runner_id`, or `None` when nothing is leasable
    /// this pass.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. "Nothing to do" is
    /// `Ok(None)`, not an error — the runner backs off and re-polls.
    ///
    /// `held` names the fleets whose sandboxes the runner says it holds; a
    /// ready one among them is tried before the partition, whichever partition
    /// it sits in.
    pub async fn select(
        &self,
        runner_id: &Uuid7,
        held: &[Uuid7],
        now: UnixMillis,
    ) -> Result<Option<Acquired>> {
        self.select_recording(runner_id, held, now).await.0
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
        held: &[Uuid7],
        now: UnixMillis,
    ) -> (Result<Option<Acquired>>, PollCost) {
        let mut cost = PollCost::default();
        let selected = self.select_counted(runner_id, held, now, &mut cost).await;
        // On EVERY exit path, including the one where the peek itself failed:
        // a poll that could not read the index is still a poll, and a total
        // that skipped it would make idle cost look lower than it is.
        producers::fleet::lease_polled(
            cost.candidates_scanned,
            cost.database_roundtrips,
            cost.dragonfly_commands,
        );
        (selected, cost)
    }

    /// [`Leases::select`] without the recording, tallying what it cost.
    async fn select_counted(
        &self,
        runner_id: &Uuid7,
        held: &[Uuid7],
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        if let Some(acquired) = self.held_first(runner_id, held, now, cost).await? {
            return Ok(Some(acquired));
        }
        let (offer, hint) = self.ready_this_poll(runner_id, cost).await;
        let acquired = self.offered(runner_id, offer, now, cost).await;
        // On every exit, a failed one included: when the fault was the hinted
        // fleet's, keeping its entry would send every later poll back to it. A
        // fault elsewhere, a Postgres error say, spends the entry too; the
        // rotation still reaches the hinted fleets in its turn.
        let leased = acquired.as_ref().ok().and_then(Option::as_ref);
        self.spend_hint(runner_id, hint, leased, cost).await;
        acquired
    }

    /// The eligible fleets among `ready`, those in `cursor` first, then in
    /// the query's own sticky order.
    ///
    /// The ordering must come from the statement and not from the peek, because
    /// sticky preference lives in its `ORDER BY`. `now` is what the held-slot
    /// filter compares a claim's expiry against, the same instant the claim
    /// itself would.
    async fn candidates(
        &self,
        runner_id: &Uuid7,
        ready: &[&str],
        cursor_ids: &[&str],
        now: UnixMillis,
    ) -> Result<Vec<Uuid7>> {
        let mut connection = self.pool().acquire().await?;
        let rows = sqlx::query(sql::lease::SELECT_READY_CANDIDATES)
            .bind(sql::FLEET_STATUS_ACTIVE)
            .bind(runner_id.as_str())
            .bind(ready)
            .bind(i64::try_from(MAX_READY_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
            .bind(now.as_millis())
            .bind(RUNNER_OFFLINE_AFTER_MS)
            .bind(sql::ADMIN_STATE_ACTIVE)
            .bind(cursor_ids)
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
}

/// Each candidate beside the generation its fleet's peek saw.
///
/// The query only narrows the peek, so every candidate has one; the pairing
/// keeps the query's order: the cursor's partition first, then sticky.
fn peeked(candidates: Vec<Uuid7>, ready: &[Ready]) -> impl Iterator<Item = (Uuid7, &ReadyToken)> {
    candidates.into_iter().filter_map(move |fleet| {
        ready
            .iter()
            .find(|entry| entry.fleet_id == fleet.as_str())
            .map(|entry| (fleet, &entry.token))
    })
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
