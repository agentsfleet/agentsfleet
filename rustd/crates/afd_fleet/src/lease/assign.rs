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
use afd_observability::producers;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::timing::LEASE_TTL_MS;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::lease::envelope::{Acquired, from_fresh, from_reclaim};
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_CANDIDATES: &str = "lease candidate scan";

/// The `core.fleets.status` value a leasable fleet carries.
pub(crate) const FLEET_STATUS_ACTIVE: &str = "active";

/// How many ready fleets one poll will consider.
///
/// `constants.zig`'s `MAX_READY_CANDIDATES_PER_POLL`. The ceiling is what makes
/// per-poll cost independent of how many fleets exist — without it a runner
/// polling an idle deployment pays for every fleet on it, every second.
const MAX_READY_CANDIDATES_PER_POLL: usize = 64;

/// The readiness index would not answer.
///
/// These five are `LOGGING_STANDARD.md` §3 `event` values — `snake_case`
/// `verb_noun`, one declaration each (RULE UFS), and byte-identical to the
/// spellings `assign.zig` emits so a dashboard built against the Zig daemon
/// keeps matching after the cutover.
const EVENT_READY_PEEK_FAILED: &str = "assign_ready_peek_failed";

/// The consumer's own pending list would not answer.
const EVENT_PEL_READ_FAILED: &str = "assign_pel_read_failed";

/// The fleet stream would not answer.
const EVENT_STREAM_READ_FAILED: &str = "assign_xreadgroup_failed";

/// An entry this consumer already held came back.
const EVENT_PEL_REDELIVERED: &str = "assign_pel_redelivered";

/// A lapsed holder's event was taken back under a higher fence.
const EVENT_LEASE_RECLAIMED: &str = "lease_reclaimed";

/// Reports a queue failure that ended a poll before any fleet was examined.
///
/// A `warn` rather than an `err` because the runner recovers on its own: it
/// backs off and re-polls, and the work stays leasable. It is emitted rather
/// than left to the caller because `LOGGING_STANDARD.md` §4 is explicit that a
/// path which can fail logs its failure — and this one propagates, so without
/// this line the only record would be whatever the handler chose to say.
fn warn_queue(event: &'static str, runner_id: &Uuid7, error: &afd_redis::Error) {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let runner = runner_id.as_str();
    let reason = error.to_string();
    tracing::warn!(
        error_code = code,
        event,
        runner_id = runner,
        reason,
        "the lease poll ended early; the runner backs off and re-polls"
    );
}

/// Reports a queue failure against one fleet's stream.
fn warn_queue_fleet(event: &'static str, fleet_id: &str, error: &afd_redis::Error) {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = error.to_string();
    tracing::warn!(
        error_code = code,
        event,
        fleet_id,
        reason,
        "the fleet's stream could not be read; its claim is not converted to a lease"
    );
}

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
        let mut cost = PollCost::default();
        let selected = self.select_counted(runner_id, now, &mut cost).await;
        // On EVERY exit path, including the one where the peek itself failed:
        // a poll that could not read the index is still a poll, and a total
        // that skipped it would make idle cost look lower than it is.
        producers::fleet::lease_polled(cost.candidates_scanned, cost.database_roundtrips);
        selected
    }

    /// [`Leases::select`] without the recording, tallying what it cost.
    async fn select_counted(
        &self,
        runner_id: &Uuid7,
        now: UnixMillis,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let ready = self
            .ready()
            .peek(MAX_READY_CANDIDATES_PER_POLL)
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
        let consumer = runner_consumer();
        let fleet = fleet_id.as_str();
        // A failed pending read cannot PROVE the pending list is empty, so it
        // must not fall through to the fresh read — promoting a new entry over
        // a possibly-pending re-poll would break own-pending-first ordering
        // exactly when Redis is degraded. Propagating is what stops it.
        let pending = streams
            .read_pending(fleet, &consumer)
            .await
            .inspect_err(|error| warn_queue_fleet(EVENT_PEL_READ_FAILED, fleet, error))?;
        let event = match pending {
            Some(event) => {
                let id = event.id.as_str();
                tracing::debug!(
                    event = EVENT_PEL_REDELIVERED,
                    fleet_id = fleet,
                    agentsfleet_event_id = id,
                    "an entry this consumer already held is being re-delivered"
                );
                Some(event)
            }
            None => streams
                .read_new(fleet, &consumer)
                .await
                .inspect_err(|error| warn_queue_fleet(EVENT_STREAM_READ_FAILED, fleet, error))?,
        };

        let Some(event) = event else {
            // Both reads answered, and both were empty — the only evidence this
            // code ever has that a fleet holds nothing deliverable. Free the
            // claim so the next event is not blocked behind it.
            self.release(fleet_id, claimed.fence, now).await?;
            return Ok(None);
        };
        from_fresh(fleet_id, claimed, &event).map(Some)
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
