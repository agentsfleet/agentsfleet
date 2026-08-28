//! `fleet.runner_affinity` — the per-fleet lease SLOT: the atomic claim, the
//! monotonic fencing source, and the sticky-routing hint, all on one row.
//!
//! `affinity.zig` is the original. What is ported is the GUARANTEE — exactly
//! one of N racing runners wins a fleet, and the winner carries a token that is
//! strictly greater than every token issued before it. Two things are
//! deliberately NOT ported. Zig answers with a `union(enum) { won, taken }`
//! because it has no nullable struct return, where here "no slot won" is an
//! absence and says so as [`Option::None`]. And Zig passes a `*pg.Conn` into
//! each function because the caller is the only thing that can own one — here
//! the verbs are methods on [`Leases`], which owns the pool and keeps it
//! `pub(crate)`, so nothing outside this crate can run a statement that is not
//! in [`crate::sql`].
//!
//! # Why the token is a type
//!
//! A lease row binds five `i64`s in a row — `event_created_at`,
//! `fencing_token`, `leased_until`, and two instants — and two of them
//! transposed compiles clean and writes a lease that can never be reported
//! against. [`Fence`] exists so that only the value a claim minted can reach
//! the column that fences reports, and §3 can demand one by type rather than by
//! comment.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_CLAIM: &str = "affinity claim";

/// Statement name, for the context a query failure carries.
const CONTEXT_RELEASE: &str = "affinity release";

/// Statement name, for the context a query failure carries.
const CONTEXT_RESET: &str = "affinity meter reset";

/// The `fencing_seq` column, which is the one number that orders lease holders.
///
/// Monotonic per fleet and minted only by [`Leases::claim`]: every winning
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

    /// The token as the wire spells it.
    ///
    /// `fencing_seq` is a monotonic counter the claim statement starts at one
    /// and only ever increments, so no value this crate produces is negative.
    /// One edited out of band could be, and it saturates to ZERO rather than
    /// panicking or taking its magnitude: zero is below every token a claim
    /// can mint, so a corrupted row fences itself out. `unsigned_abs` alone
    /// would turn `-1` into `1`, which is a token another holder may legitimately
    /// hold — a plausible wrong answer, which is the worse failure.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        if self.0 < 0 { 0 } else { self.0.unsigned_abs() }
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

/// A won claim: the new token, and the instant the slot — and the lease issued
/// against it — stays valid until.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claimed {
    /// The monotonic token this claim minted.
    pub fence: Fence,
    /// When the claim lapses and the slot becomes winnable again.
    pub leased_until: UnixMillis,
}

impl Leases {
    /// Atomically claim `fleet_id`'s lease slot for `runner_id`, valid for
    /// `ttl_ms`.
    ///
    /// Wins iff the slot is unclaimed or its prior claim has expired, bumping
    /// the monotonic token and recording the sticky hint. Answers `None` when a
    /// live runner still holds it — an ordinary outcome the caller reads as
    /// "try the next candidate", not a failure.
    ///
    /// The claim PRECEDES the event read by design: a loser has consumed no
    /// event, so nothing is orphaned by losing.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A refused claim is
    /// `Ok(None)`.
    pub async fn claim(
        &self,
        fleet_id: &Uuid7,
        runner_id: &Uuid7,
        now: UnixMillis,
        ttl_ms: i64,
    ) -> Result<Option<Claimed>> {
        let leased_until = now.saturating_add_millis(ttl_ms);
        let mut connection = self.pool().acquire().await?;
        let won = sqlx::query(sql::lease::CLAIM_AFFINITY_SLOT)
            .bind(fleet_id.as_str())
            .bind(runner_id.as_str())
            .bind(leased_until.as_millis())
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_CLAIM))?;

        // No row is the `.taken` verdict: the conditional UPSERT's WHERE clause
        // declined to update, which means someone else's claim is still live.
        let Some(row) = won else {
            return Ok(None);
        };
        let fence: i64 = row.try_get(0).map_err(query(CONTEXT_CLAIM))?;
        Ok(Some(Claimed {
            fence: Fence::from_i64(fence),
            leased_until,
        }))
    }

    /// Free the slot so the fleet's next event is claimable.
    ///
    /// Token-guarded: frees it only while `fence` is still the live token, so a
    /// holder superseded by a reclaim cannot free the CURRENT holder's slot and
    /// hand one fleet to two runners. Idempotent — a no-op when the row is gone
    /// or the token has moved on.
    ///
    /// Called on every post-claim path that does not issue a lease, so an
    /// abandoned claim costs one poll rather than a full TTL of silence on that
    /// fleet.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn release(&self, fleet_id: &Uuid7, fence: Fence, now: UnixMillis) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::lease::RELEASE_AFFINITY_SLOT)
            .bind(fleet_id.as_str())
            .bind(now.as_millis())
            .bind(fence.as_i64())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }

    /// Reset the slot's metering cursor to zero at a FRESH lease issue.
    ///
    /// A reclaim must NOT call this: the slot has to keep the dead holder's
    /// progress so the re-leased run meters forward from where it stopped,
    /// which is exactly why the cursor is absent from the claim's `ON CONFLICT`
    /// SET.
    ///
    /// Fail-closed by contract — the caller treats an error here as a failed
    /// lease issue rather than a warning, because the renewal CTE reads this
    /// cursor for each slice's delta and a stale value would over-charge the
    /// first renewal.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn reset_meters(&self, fleet_id: &Uuid7, now: UnixMillis) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::lease::RESET_AFFINITY_METERS)
            .bind(fleet_id.as_str())
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RESET))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fence only ever moves through the column's own type.
    ///
    /// Cheap, and it is the property that makes the newtype worth its weight:
    /// what goes into the row is what came out of the claim.
    #[test]
    fn test_a_fence_round_trips_through_its_column_type() {
        let fence = Fence::from_i64(7);
        assert_eq!(fence.as_i64(), 7, "the token reaches the column unchanged");
    }

    /// Fences order, because that ordering IS the staleness test.
    ///
    /// §3 rejects a report whose token is behind the slot's current one, so an
    /// ordering that did not hold would be a stale writer admitted.
    #[test]
    fn test_a_later_fence_outranks_an_earlier_one() {
        assert!(
            Fence::from_i64(2) > Fence::from_i64(1),
            "a reclaim's token must outrank the holder it displaced"
        );
    }
}
