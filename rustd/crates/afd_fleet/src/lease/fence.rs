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
//! is negative. One edited out of band could be, and `liveLeaseSeq` guards it
//! with an explicit `if (raw < 0) return error.InvalidFencingSeq` because Zig's
//! `@intCast` would TRAP and take the daemon down. [`u64::try_from`] is the
//! same check without the trap to avoid, and its `Err` is the same refusal.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{Result, query, sequence_corrupt};
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_FENCE: &str = "live fence lookup";

/// The fleet's live fencing sequence, if this runner holds a live lease on it.
///
/// `COALESCE(a.fencing_seq, l.fencing_token)` so a reclaim that bumped the
/// sequence strands the old holder BELOW it — the affinity row is the live
/// authority and the lease's own token is only the fallback for a fleet whose
/// slot row is gone.
///
/// `$1` runner, `$2` fleet, `$3` the active status, `$4` now.
const SELECT_LIVE_FENCE_BY_FLEET: &str = "\
SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.runner_id = $1::uuid AND l.fleet_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > $4
ORDER BY l.created_at DESC
LIMIT 1";

/// The same fence, addressed by lease id when the caller already holds one.
///
/// Keyed by lease AND fleet, so a lease that exists but belongs to another
/// fleet yields no row — the IDOR cross-check IS the `WHERE`, not a comparison
/// the handler has to remember to make afterwards.
///
/// `$1` lease, `$2` runner, `$3` fleet, `$4` the active status, `$5` now.
const SELECT_LIVE_FENCE_BY_LEASE: &str = "\
SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.fleet_id = $3::uuid
  AND l.status = $4 AND l.lease_expires_at > $5
LIMIT 1";

impl Leases {
    /// The fleet's live fencing sequence, if `runner_id` holds a live lease on it.
    ///
    /// `None` means no live lease — expired, reclaimed, or never held. That is
    /// the authorization answer for a hydrate: a runner may read a fleet's
    /// memory only while it is actually running it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored sequence that is
    /// not a sequence.
    pub async fn live_fence_for_fleet(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<u64>> {
        let found = sqlx::query(SELECT_LIVE_FENCE_BY_FLEET)
            .bind(runner_id.as_str())
            .bind(fleet_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis());
        self.read_fence(found).await
    }

    /// The same sequence, for a caller naming the lease it believes it holds.
    ///
    /// Keyed by lease AND fleet, so a lease belonging to another fleet answers
    /// `None` — the cross-check that stops a runner reaching one fleet's memory
    /// with another fleet's lease is the statement's `WHERE`, not a comparison
    /// this code has to remember to make.
    ///
    /// # Errors
    /// As [`Leases::live_fence_for_fleet`].
    pub async fn live_fence_for_lease(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<u64>> {
        let found = sqlx::query(SELECT_LIVE_FENCE_BY_LEASE)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .bind(fleet_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis());
        self.read_fence(found).await
    }

    /// Run a prepared fence statement and widen its column.
    async fn read_fence(
        &self,
        statement: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> Result<Option<u64>> {
        let mut connection = self.pool().acquire().await?;
        let Some(row) = statement
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_FENCE))?
        else {
            return Ok(None);
        };
        let stored: i64 = row.try_get(0).map_err(query(CONTEXT_FENCE))?;
        u64::try_from(stored)
            .map(Some)
            .map_err(|_negative| sequence_corrupt())
    }
}
