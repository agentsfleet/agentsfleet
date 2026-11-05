//! `runner.fleet_slots` — the per-fleet lease SLOT: the monotonic fencing
//! source, the sticky-routing hint and the queue position, all on one row.
//!
//! The claim itself is the assignment statement (`super::assign`); this
//! module owns what a claim hands out and how a claim that issued no lease is
//! let go. The verbs are methods on [`Leases`], which owns the pool and keeps
//! it `pub(crate)`, so nothing outside this crate can run a statement that is
//! not in [`crate::lease::sql`].
//!
//! # Why the token is a type
//!
//! A lease row binds five `i64`s in a row — `event_created_at`,
//! `fencing_token`, `lease_expires_at`, and two instants — and two of them
//! transposed compiles clean and writes a lease that can never be reported
//! against. [`Fence`] exists so that only the value a claim minted can reach
//! the column that fences reports, and §3 can demand one by type rather than by
//! comment.

use afd_core::error_code::{Coded as _, Logged};
use afd_core::id::Uuid7;
use afd_core::timing::{CLAIM_RETRY_BACKOFF, SANDBOX_HOLD_IDLE};
use afd_observability::metrics::label::fleet::HeldClaim;
use afd_observability::producers;
use jiff::Timestamp;
use sqlx::PgConnection;
use std::time::Duration;

use crate::error::{Result, query};
use crate::lease::envelope::Acquired;
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_RELEASE: &str = "affinity release";

/// A claim that issued no lease could not be freed; it lapses at its expiry.
pub(crate) const EVENT_CLAIM_RELEASE_FAILED: &str = "lease_claim_release_failed";

/// The `fencing_token` column, which is the one number that orders lease holders.
///
/// Monotonic per fleet and minted only by the lease claim: every winning
/// claim bumps it, so a holder superseded by a reclaim carries a strictly
/// smaller value than the runner that displaced it. That is the whole basis of
/// Invariant 2, and the reason a report verifies its token inside the same
/// statement that flips the lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fence(i64);

impl Fence {
    /// The token as the column stores it.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    /// The token as the wire spells it, or `None` for a negative one.
    ///
    /// `fencing_token` is a monotonic counter: a slot starts at zero and every
    /// claim increments it, so no value this crate produces is negative. One
    /// edited out of band could be, and it is refused rather than mapped: zero
    /// is a slot no claim has taken yet, and `unsigned_abs` would turn `-1`
    /// into `1`, a token another holder may legitimately hold. Either is a
    /// plausible wrong answer, which is the worse failure.
    #[must_use]
    pub const fn as_u64(self) -> Option<u64> {
        if self.0 < 0 { None } else { Some(self.0.unsigned_abs()) }
    }

    /// A token as read back from a row.
    ///
    /// `pub(crate)` on purpose: outside this crate a `Fence` can only come from
    /// a claim or from a row this crate read, never from arithmetic on an
    /// `i64` a caller happened to have.
    pub(crate) const fn from_i64(value: i64) -> Self {
        Self(value)
    }
}

/// How long a freed claim keeps its fleet out of the next claims.
///
/// The claim takes the OLDEST ready fleet first, so a fleet whose pass ended
/// for a reason the next poll would meet again would sit at the head of every
/// poll and starve the fleets behind it. Such an ending frees the slot a little
/// in the future instead of in the past: `ready_at` keeps the fleet's place in
/// the queue, and the claim's `lease_expires_at < now` predicate skips it until
/// the backoff passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backoff {
    /// Free at once, for another runner: the fleet itself is fine.
    None,
    /// Free after [`CLAIM_RETRY_BACKOFF`]: the pass met a fault or a gate
    /// that could not decide, and the next poll would likely meet it again.
    Retry,
}

impl Backoff {
    /// The interval the release statement adds to its clock.
    const fn delay(self) -> Duration {
        match self {
            Self::None => Duration::ZERO,
            Self::Retry => CLAIM_RETRY_BACKOFF,
        }
    }
}

impl Leases {
    /// Free a slot whose claim issued no lease, so the fleet's next event is
    /// claimable.
    ///
    /// Token-guarded: frees it only while `fence` is still the live token, so a
    /// holder superseded by a reclaim cannot free the CURRENT holder's slot and
    /// hand one fleet to two runners. Idempotent — a no-op when the row is gone
    /// or the token has moved on.
    ///
    /// Called on every post-claim path that does not issue a lease, so an
    /// abandoned claim costs one poll rather than a full TTL of silence on that
    /// fleet, or `backoff` when the ending would repeat on the next poll. It
    /// also drops the sticky hint the claim wrote: no run happened on this
    /// runner, and a hint left behind would sort a fleet that keeps stopping
    /// first for it on every poll.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn release(&self, fleet_id: &Uuid7, fence: Fence) -> Result<()> {
        self.release_after(fleet_id, fence, Backoff::None).await
    }

    /// [`Leases::release`], keeping the fleet out of claims for `backoff`.
    pub(crate) async fn release_after(
        &self,
        fleet_id: &Uuid7,
        fence: Fence,
        backoff: Backoff,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        self.tally().statements(1);
        sqlx::query(sql::lease::RELEASE_UNLEASED_SLOT)
            .bind(fleet_id.as_str())
            .bind(fence.as_i64())
            .bind(backoff.delay())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }

    /// Frees a claim that issued no lease, best-effort.
    ///
    /// The one place every post-claim ending without a lease lets go: the
    /// assignment pass's faults, and the pull's refusals, retries, parks and
    /// faults. The caller already has its answer, and a release that fails
    /// changes nothing it can act on: the claim lapses at its expiry, which is
    /// the cost every such ending paid before this existed. So the failure is
    /// logged and the answer stands.
    pub(crate) async fn let_go(&self, fleet_id: &Uuid7, fence: Fence, backoff: Backoff) {
        if let Err(failure) = self.release_after(fleet_id, fence, backoff).await {
            let Logged { error_code, reason } = failure.logged();
            let fleet_id = fleet_id.as_str();
            tracing::warn!(
                error_code,
                event = EVENT_CLAIM_RELEASE_FAILED,
                fleet_id,
                reason,
                "a claim that issued no lease was not freed; it lapses at its expiry"
            );
        }
    }

    /// The release a finished run owes, on a connection the caller already
    /// holds. Keeps the sticky hint: this runner did lease the fleet. Records
    /// `hold_expires_at` too, the hold the run's sandbox is kept under, or clears
    /// it: the fencing guard means a superseded holder records nothing.
    ///
    /// The report path needs it: freeing the slot makes the fleet's next event
    /// claimable, and doing that before the run's result is durable would let a
    /// fresh lease race a half-written finalize. Inside the settle's
    /// transaction there is no half-written state to race — the freed slot
    /// becomes visible at the same instant the terminal row does — which is
    /// what lets this be a guarantee rather than the ordering comment it was.
    ///
    /// It takes a connection so it adds no acquire (RULE CNX).
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn release_through(
        &self,
        connection: &mut PgConnection,
        fleet_id: &Uuid7,
        fence: Fence,
        asked_hold: Option<Timestamp>,
    ) -> Result<()> {
        sqlx::query(sql::lease::RELEASE_FLEET_SLOT)
            .bind(fleet_id.as_str())
            .bind(fence.as_i64())
            .bind(asked_hold.map(jiff_sqlx::ToSqlx::to_sqlx))
            .bind(SANDBOX_HOLD_IDLE)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }
}

#[cfg(feature = "test-util")]
impl Leases {
    /// Steers `fleet_id`'s next claim to `runner_id`, as a finished run on
    /// that runner would: the slot names it the fleet's last runner and holds
    /// no lease. Test fixtures share the lane database, and the claim's
    /// affinity order is how a suite keeps its runner on its own fleet.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn prefer(&self, fleet_id: &Uuid7, runner_id: &Uuid7) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::lease::PREFER_RUNNER)
            .bind(fleet_id.as_str())
            .bind(runner_id.as_str())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }
}

/// Counts the holder's claim on its live hold, on the exit that hands out a
/// lease resuming it. Not at the claim: the holder's reclaim resumes nothing,
/// since the event's first attempt may have run in that sandbox.
pub(crate) fn count_resumed(acquired: &Acquired) {
    if acquired.resume_hold {
        producers::fleet::hold::claimed(HeldClaim::Holder);
    }
}

/// Who won a claim on a fleet whose slot held a sandbox live at `now`, from
/// the slot's prior `hold_expires_at` and `last_runner_id`, or `None` when it held
/// none. A hold at or past its deadline binds nobody at the claim's own
/// predicate, so it is no hold here either.
pub(crate) fn held_claim(
    held: bool,
    holder: Option<&str>,
    runner_id: &Uuid7,
) -> Option<HeldClaim> {
    held.then(|| {
        if holder == Some(runner_id.as_str()) {
            HeldClaim::Holder
        } else {
            HeldClaim::OtherAfterLapse
        }
    })
}

#[cfg(test)]
#[path = "affinity_tests.rs"]
mod tests;
