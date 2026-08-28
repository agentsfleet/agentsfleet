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
use crate::lease::store::Leases;
use crate::lease::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_FENCE: &str = "live fence lookup";

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
        let found = sqlx::query(crate::memory::sql::SELECT_LIVE_FENCE_BY_FLEET)
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
        let found = sqlx::query(crate::memory::sql::SELECT_LIVE_FENCE_BY_LEASE)
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
