//! Proving a runner holds a live lease on a fleet, and what token it holds.
//!
//! The memory verbs authorize differently from every other verb in this plane.
//! A report names its LEASE and the statement finds the fleet; a memory call
//! names its FLEET — the runner already holds it in the lease payload, so
//! naming it beats inferring it from ambient state — and the statement has to
//! find the lease.
//!
//! # The token is `u64`, and the column is not
//!
//! `fencing_seq` is server-issued and monotonic, so no value this daemon writes
//! is negative. One edited out of band could be, and [`u64::try_from`] refuses
//! it: its `Err` becomes the `sequence_corrupt` refusal rather than a token
//! that wrapped to a huge positive value.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{Result, query, sequence_corrupt};
use crate::lease::sql;
use crate::lease::standing::{fence_current, fence_holds};
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_FENCE: &str = "live fence lookup";

/// The lease's own token and the fleet's live fencing sequence, if this runner
/// holds a live lease on it.
///
/// `COALESCE(a.fencing_seq, l.fencing_token)` so a reclaim that bumped the
/// sequence strands the old holder BELOW it — the affinity row is the live
/// authority and the lease's own token is only the fallback for a fleet whose
/// slot row is gone.
///
/// Where one runner briefly holds two live leases on the fleet — a superseded
/// one and its successor — the highest token wins: tokens rise with every
/// issue, where `created_at` is one daemon replica's clock.
///
/// `$1` runner, `$2` fleet, `$3` the active status, `$4` now.
const SELECT_LIVE_FENCE_BY_FLEET: &str = "\
SELECT l.fencing_token AS own, COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.runner_id = $1::uuid AND l.fleet_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > $4
ORDER BY l.fencing_token DESC
LIMIT 1";

/// The same fence, addressed by lease id when the caller already holds one.
///
/// Keyed by lease AND fleet, so a lease that exists but belongs to another
/// fleet yields no row — the IDOR cross-check IS the `WHERE`, not a comparison
/// the handler has to remember to make afterwards.
///
/// `$1` lease, `$2` runner, `$3` fleet, `$4` the active status, `$5` now.
const SELECT_LIVE_FENCE_BY_LEASE: &str = "\
SELECT l.fencing_token AS own, COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.fleet_id = $3::uuid
  AND l.status = $4 AND l.lease_expires_at > $5
LIMIT 1";

/// A live lease's own token beside its fleet's live sequence.
///
/// Both, because neither alone decides. A lease row stays `active` after a
/// reclaim has moved the fleet past it, until the reclaim's issue expires it,
/// so "the lease is live" says nothing about whether it still holds the fleet;
/// and a token compared only against the sequence admits any value above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fence {
    /// The token the lease was issued with.
    own: i64,
    /// The fleet's live sequence.
    live_seq: i64,
}

impl Fence {
    /// The two columns a fence read answers, refused when either cannot be a
    /// sequence: a negative live sequence has no safe reading (see
    /// [`sequence_corrupt`]), and a negative token is no lease's.
    fn new(own: i64, live_seq: i64) -> Result<Self> {
        if own < 0 || live_seq < 0 {
            return Err(sequence_corrupt());
        }
        Ok(Self { own, live_seq })
    }

    /// Whether no reclaim has moved the fleet past this lease: the first half
    /// of the rule [`Self::holds`] applies, from the same function.
    pub(crate) const fn current(self) -> bool {
        fence_current(self.own, self.live_seq)
    }

    /// Whether this lease still holds the fleet and `presented` is its own
    /// token: the rule report and renew apply, through the same function.
    pub(crate) fn holds(self, presented: u64) -> bool {
        fence_holds(self.own, self.live_seq, presented)
    }

    /// The lease's own token, for the line a refusal logs.
    pub(crate) const fn own(self) -> i64 {
        self.own
    }

    /// The fleet's live sequence, for the line a refusal logs.
    pub(crate) const fn live_seq(self) -> i64 {
        self.live_seq
    }
}

impl Leases {
    /// The lease's fence, if `runner_id` holds a live lease on the fleet.
    ///
    /// `None` means no live lease — expired, reclaimed, or never held. A lease
    /// that is live but superseded answers a [`Fence`] that is not
    /// [`current`](Fence::current); a hydrate refuses both, because a runner
    /// may read a fleet's memory only while it is actually running it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored sequence that is
    /// not a sequence.
    pub(crate) async fn live_fence_for_fleet(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Fence>> {
        let found = sqlx::query(SELECT_LIVE_FENCE_BY_FLEET)
            .bind(runner_id.as_str())
            .bind(fleet_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis());
        self.read_fence(found).await
    }

    /// The same fence, for a caller naming the lease it believes it holds.
    ///
    /// Keyed by lease AND fleet, so a lease belonging to another fleet answers
    /// `None` — the cross-check that stops a runner reaching one fleet's memory
    /// with another fleet's lease is the statement's `WHERE`, not a comparison
    /// this code has to remember to make.
    ///
    /// # Errors
    /// As [`Leases::live_fence_for_fleet`].
    pub(crate) async fn live_fence_for_lease(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Fence>> {
        let found = sqlx::query(SELECT_LIVE_FENCE_BY_LEASE)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .bind(fleet_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis());
        self.read_fence(found).await
    }

    /// Run a prepared fence statement and check its columns are sequences.
    async fn read_fence(
        &self,
        statement: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> Result<Option<Fence>> {
        let mut connection = self.pool().acquire().await?;
        let Some(row) = statement
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_FENCE))?
        else {
            return Ok(None);
        };
        let own: i64 = row.try_get(0).map_err(query(CONTEXT_FENCE))?;
        let live_seq: i64 = row.try_get(1).map_err(query(CONTEXT_FENCE))?;
        Fence::new(own, live_seq).map(Some)
    }
}

#[cfg(test)]
#[path = "fence/tests.rs"]
mod tests;
