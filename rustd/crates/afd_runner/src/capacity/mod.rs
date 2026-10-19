//! Serializes fleet and authoring leases on the physical host's assigned limit.
//!
//! Callers retain the transaction through the guarded lease write. The census
//! is a separate statement after the host lock: a statement snapshot acquired
//! before waiting for that lock could miss the preceding claimant's commit.

use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::{Postgres, Transaction};

use crate::error::{Result, query};

pub mod choice;
mod sql;

/// Capacity observed while holding the host row until transaction completion.
#[derive(Debug, Clone, Copy)]
pub struct Capacity {
    now: Timestamp,
    workers: i32,
    occupied: i64,
    claims_allowed: bool,
    renewals_allowed: bool,
    lease_version: i64,
}

impl Capacity {
    /// Authoritative database time, read once the host lock was held.
    #[must_use]
    pub const fn now(self) -> Timestamp {
        self.now
    }

    /// Whether one additional lease fits the currently assigned worker limit.
    #[must_use]
    pub fn can_claim(self) -> bool {
        self.claims_allowed && self.occupied < i64::from(self.workers)
    }

    /// A renewal retains an already counted, unexpired slot.
    #[must_use]
    pub fn can_renew(self) -> bool {
        self.renewals_allowed && self.occupied <= i64::from(self.workers)
    }
}

/// [`sql::LOCK_HOST`]'s row.
#[derive(Debug, sqlx::FromRow)]
struct Host {
    worker_count: i32,
    status: String,
    degraded: bool,
    lease_version: i64,
}

/// [`sql::CENSUS`]'s row.
#[derive(Debug, sqlx::FromRow)]
struct Census {
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    checked_at: Timestamp,
    occupied: i64,
}

/// Lock a host and count both kinds using a fresh database-time snapshot.
///
/// `None` means the authenticated host was removed. Cordoned/draining hosts
/// may finish current work but cannot claim more; revoked/degraded hosts do
/// neither. Lease-specific original-expiry/fence checks remain mandatory.
///
/// # Errors
/// Preserves statement and column failures with their query context.
pub async fn lock(
    transaction: &mut Transaction<'_, Postgres>,
    runner_id: &Uuid7,
) -> Result<Option<Capacity>> {
    let host = sqlx::query_as::<_, Host>(sql::LOCK_HOST)
        .bind(runner_id.as_str())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    let Some(host) = host else { return Ok(None) };
    let census = sqlx::query_as::<_, Census>(sql::CENSUS)
        .bind(runner_id.as_str())
        .bind(crate::sql::LEASE_STATUS_ACTIVE)
        .fetch_one(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(Some(Capacity {
        now: census.checked_at,
        workers: host.worker_count,
        occupied: census.occupied,
        claims_allowed: !host.degraded && host.status == crate::sql::RUNNER_STATUS_ACTIVE,
        renewals_allowed: !host.degraded && renewable_state(&host.status),
        lease_version: host.lease_version,
    }))
}

fn renewable_state(state: &str) -> bool {
    use afd_api_wire::admin::AdminState;
    matches!(
        afd_core::spelling::from_spelling::<AdminState>(state),
        Some(AdminState::Active | AdminState::Cordoned | AdminState::Draining)
    )
}
