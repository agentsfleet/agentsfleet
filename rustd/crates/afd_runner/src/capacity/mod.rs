//! Serializes fleet and authoring leases on the physical host's assigned limit.
//!
//! Callers retain the transaction through the guarded lease write. The census
//! is a separate statement after the host lock: a statement snapshot acquired
//! before waiting for that lock could miss the preceding claimant's commit.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::{Postgres, Row as _, Transaction};

use crate::error::{Result, query};

pub mod choice;
mod sql;

/// Capacity observed while holding the host row until transaction completion.
#[derive(Debug, Clone, Copy)]
pub struct Capacity {
    now: UnixMillis,
    workers: i32,
    occupied: i64,
    claims_allowed: bool,
    renewals_allowed: bool,
    claim_revision: i64,
}

impl Capacity {
    /// Authoritative database time sampled after acquiring the host lock.
    #[must_use]
    pub const fn now(self) -> UnixMillis {
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
    let host = sqlx::query(sql::LOCK_HOST)
        .bind(runner_id.as_str())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    let Some(host) = host else { return Ok(None) };
    let workers: i32 = host.try_get(0).map_err(query(sql::CONTEXT))?;
    let state: String = host.try_get(1).map_err(query(sql::CONTEXT))?;
    let degraded: bool = host.try_get(2).map_err(query(sql::CONTEXT))?;
    let census = sqlx::query(sql::CENSUS)
        .bind(runner_id.as_str())
        .bind(crate::sql::LEASE_STATUS_ACTIVE)
        .fetch_one(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(Some(Capacity {
        now: UnixMillis::from_millis(census.try_get(0).map_err(query(sql::CONTEXT))?),
        workers,
        occupied: census.try_get(1).map_err(query(sql::CONTEXT))?,
        claims_allowed: !degraded && state == crate::sql::ADMIN_STATE_ACTIVE,
        renewals_allowed: !degraded && renewable_state(&state),
        claim_revision: host.try_get(3).map_err(query(sql::CONTEXT))?,
    }))
}

fn renewable_state(state: &str) -> bool {
    use afd_api_wire::admin::AdminState;
    matches!(
        afd_core::spelling::from_spelling::<AdminState>(state),
        Some(AdminState::Active | AdminState::Cordoned | AdminState::Draining)
    )
}
