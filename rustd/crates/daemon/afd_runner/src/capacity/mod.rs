//! Serializes fleet and authoring leases on the physical host's assigned limit.
//!
//! Callers retain the transaction through the guarded lease write. The census
//! is a separate statement after the host lock: a statement snapshot acquired
//! before waiting for that lock could miss the preceding claimant's commit.

use af_core::id::Uuid7;
use afd_auth::directory::Digest;
use jiff::Timestamp;
use sqlx::{Postgres, Transaction};

use crate::error::{Result, query};

pub mod authority;
pub mod choice;
mod sql;

/// The claim owner, proved under the same host lock as the physical capacity.
pub enum Guard<'a> {
    /// A trusted domain call for an enrolled host.
    Enrolled(&'a Uuid7),
    /// A stream call must also prove the exact opening credential and connection.
    Connected {
        /// Host whose capacity is being inspected.
        runner: &'a Uuid7,
        /// The opening's expected database owner.
        connection: &'a Uuid7,
        /// Digest proved at opening; never included in debug output.
        digest: &'a Digest,
    },
}

impl std::fmt::Debug for Guard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enrolled(runner) => f.debug_tuple("Enrolled").field(runner).finish(),
            Self::Connected {
                runner, connection, ..
            } => f
                .debug_struct("Connected")
                .field("runner", runner)
                .field("connection", connection)
                .finish_non_exhaustive(),
        }
    }
}

impl<'a> From<&'a Uuid7> for Guard<'a> {
    fn from(runner: &'a Uuid7) -> Self {
        Self::Enrolled(runner)
    }
}

impl<'a> From<&'a authority::Authority> for Guard<'a> {
    fn from(authority: &'a authority::Authority) -> Self {
        authority.guard_for(authority.runner())
    }
}

impl<'a> From<(&'a Uuid7, &'a authority::Authority)> for Guard<'a> {
    fn from((runner, authority): (&'a Uuid7, &'a authority::Authority)) -> Self {
        authority.guard_for(runner)
    }
}

/// Capacity observed while holding the host row until transaction completion.
#[derive(Debug, Clone, Copy)]
pub struct Capacity {
    now: Timestamp,
    workers: i32,
    occupied: i64,
    claims_allowed: bool,
    renewals_allowed: bool,
    lease_version: i64,
    decision_selected: bool,
}

impl Capacity {
    /// Authoritative database time, read once the host lock was held.
    #[must_use]
    pub const fn now(self) -> Timestamp {
        self.now
    }

    /// Whether this locked host selected a decision model. Unconfigured hosts
    /// need no catalogue read on the ordinary lease path.
    #[must_use]
    pub const fn decision_selected(self) -> bool {
        self.decision_selected
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
    authorized: bool,
    worker_count: i32,
    status: String,
    degraded: bool,
    lease_version: i64,
    decision_selected: bool,
}

/// [`sql::CENSUS`]'s row.
#[derive(Debug, sqlx::FromRow)]
struct Census {
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    checked_at: Timestamp,
    occupied: i64,
}

/// The host row as [`lock_host`] holds it: what the claim checks before the
/// guarded lease insert counts the host's census itself (M245 §5).
#[derive(Debug, Clone, Copy)]
pub struct Held {
    claims_allowed: bool,
    lease_version: i64,
    decision_selected: bool,
}

impl Held {
    /// Whether the host may take new work: active and not degraded. Whether a
    /// worker is free is the insert's to count.
    #[must_use]
    pub const fn can_claim(self) -> bool {
        self.claims_allowed
    }

    /// The placement choice's revision, which the insert advances.
    #[must_use]
    pub const fn lease_version(self) -> i64 {
        self.lease_version
    }

    /// Whether the host has a decision profile selected.
    #[must_use]
    pub const fn decision_selected(self) -> bool {
        self.decision_selected
    }
}

/// Lock the host row and prove its owner, without the census.
///
/// For the fleet issue, whose insert counts occupied workers in its own guard:
/// that statement's snapshot is taken after this lock, so it sees the
/// preceding claimant's commit as the separate census statement did.
///
/// # Errors
/// Reports a datastore that would not answer.
pub async fn lock_host<'a>(
    transaction: &mut Transaction<'_, Postgres>,
    guard: impl Into<Guard<'a>>,
) -> Result<Option<Held>> {
    Ok(locked(transaction, guard).await?.map(|(_, host)| Held {
        claims_allowed: !host.degraded && host.status == crate::sql::RUNNER_STATUS_ACTIVE,
        lease_version: host.lease_version,
        decision_selected: host.decision_selected,
    }))
}

/// What a guard binds: the host, and for a stream call the opening's
/// credential digest and connection, which `lock_host.sql` and a host's
/// renewal statement compare against the host row alike (M245 §7).
#[must_use]
pub fn proof<'g>(guard: impl Into<Guard<'g>>) -> (&'g Uuid7, Option<&'g str>, Option<&'g Uuid7>) {
    match guard.into() {
        Guard::Enrolled(runner) => (runner, None, None),
        Guard::Connected {
            runner,
            connection,
            digest,
        } => (runner, Some(digest.as_str()), Some(connection)),
    }
}

/// The authorized host row, under its lock, and the runner it names.
async fn locked<'g>(
    transaction: &mut Transaction<'_, Postgres>,
    guard: impl Into<Guard<'g>>,
) -> Result<Option<(Uuid7, Host)>> {
    let (runner_id, digest, connection) = proof(guard);
    let host = sqlx::query_as::<_, Host>(sql::LOCK_HOST)
        .bind(runner_id)
        .bind(digest)
        .bind(connection)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(host
        .filter(|host| host.authorized)
        .map(|host| (runner_id.clone(), host)))
}

/// Lock a host and count both kinds using a fresh database-time snapshot.
///
/// `None` means the authenticated host was removed. Cordoned/draining hosts
/// may finish current work but cannot claim more; revoked/degraded hosts do
/// neither. Lease-specific original-expiry/fence checks remain mandatory.
///
/// # Errors
/// Preserves statement and column failures with their query context.
pub async fn lock<'a>(
    transaction: &mut Transaction<'_, Postgres>,
    guard: impl Into<Guard<'a>>,
) -> Result<Option<Capacity>> {
    let Some((runner_id, host)) = locked(transaction, guard).await? else {
        return Ok(None);
    };
    let census = sqlx::query_as::<_, Census>(sql::CENSUS)
        .bind(&runner_id)
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
        decision_selected: host.decision_selected,
    }))
}

/// The host states a renewal may run in: one list, read by the locked
/// capacity check and by a host's renewal statement alike (M245 §7).
const RENEWABLE: [afd_api_wire::admin::AdminState; 3] = [
    afd_api_wire::admin::AdminState::Active,
    afd_api_wire::admin::AdminState::Cordoned,
    afd_api_wire::admin::AdminState::Draining,
];

fn renewable_state(state: &str) -> bool {
    af_core::spelling::from_spelling::<afd_api_wire::admin::AdminState>(state)
        .is_some_and(|state| RENEWABLE.contains(&state))
}

/// [`RENEWABLE`]'s states spelled as `runner.runners.status` stores them, for
/// a statement that reads the host without its lock.
#[must_use]
pub fn renewable_states() -> Vec<String> {
    RENEWABLE
        .iter()
        .filter_map(af_core::spelling::to_spelling)
        .collect()
}
