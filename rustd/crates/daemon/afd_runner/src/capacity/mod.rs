//! Serializes fleet and authoring runs on the physical host's assigned limit.
//!
//! Callers retain the transaction through the guarded run write. The census
//! is a separate statement after the host lock: a statement snapshot acquired
//! before waiting for that lock could miss the preceding claimant's commit.

use af_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::{Postgres, Transaction};

use crate::error::{Result, query};

pub mod authority;
pub mod choice;
mod guard;
pub use guard::{Guard, proof};
mod sql;

/// Which work consumes a worker; this never grants connection authority.
#[derive(Debug, Clone, Copy)]
pub enum Claimant<'a> {
    /// Reusing this fleet's hold replaces it without taking another worker.
    Fleet(&'a Uuid7),
    /// Private authoring cannot reuse a fleet sandbox.
    Authoring,
    /// Current work renews its live slot without charging idle holds.
    Existing,
}

/// Whether a locked host may take the work it was locked for, and whether a
/// tenant owns it, as the host lock's statement answered both.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct Ownership {
    serves: bool,
    tenant_owned: bool,
}

/// Capacity observed while holding the host row until transaction completion.
#[derive(Debug, Clone, Copy)]
pub struct Capacity {
    now: Timestamp,
    workers: i32,
    occupied: i64,
    held: i64,
    claims_allowed: bool,
    renewals_allowed: bool,
    run_version: i64,
    decision_selected: bool,
    ownership: Ownership,
}

impl Capacity {
    /// Whether this host may take the tenant it was locked for: the platform's
    /// hosts take any tenant's work, a tenant's host only its own. Postgres
    /// answers it under the lock, with the predicate the claim uses.
    #[must_use]
    pub const fn serves(self) -> bool {
        self.ownership.serves
    }

    /// Authoritative database time, read once the host lock was held.
    #[must_use]
    pub const fn now(self) -> Timestamp {
        self.now
    }

    /// Whether this locked host selected a decision model. Unconfigured hosts
    /// need no catalogue read on the ordinary run path.
    #[must_use]
    pub const fn decision_selected(self) -> bool {
        self.decision_selected
    }

    /// Whether one additional run fits the currently assigned worker limit.
    #[must_use]
    pub fn can_claim(self) -> bool {
        self.claims_allowed && self.occupied + self.held < i64::from(self.workers)
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
    cordon_reason: Option<String>,
    run_version: i64,
    decision_selected: bool,
    #[sqlx(flatten)]
    ownership: Ownership,
}

/// [`sql::CENSUS`]'s row.
#[derive(Debug, sqlx::FromRow)]
struct Census {
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    checked_at: Timestamp,
    occupied: i64,
    held: i64,
}

/// The host row as [`lock_host`] holds it: what the claim checks before the
/// guarded run insert counts the host's census itself.
#[derive(Debug, Clone, Copy)]
pub struct Held {
    claims_allowed: bool,
    continuations_allowed: bool,
    run_version: i64,
    decision_selected: bool,
    ownership: Ownership,
}

impl Held {
    /// Whether this host may take the tenant it was locked for: the platform's
    /// hosts take any tenant's work, a tenant's host only its own. Postgres
    /// answers it under the lock, with the predicate the claim uses.
    #[must_use]
    pub const fn serves(self) -> bool {
        self.ownership.serves
    }

    /// Whether a tenant registered this host, so no platform model key may reach it.
    #[must_use]
    pub const fn tenant_owned(self) -> bool {
        self.ownership.tenant_owned
    }

    /// Whether the host may take new work: active and not degraded. Whether a
    /// worker is free is the insert's to count.
    #[must_use]
    pub const fn can_claim(self) -> bool {
        self.claims_allowed
    }

    /// Current work may continue on a cordoned or draining host, while a
    /// revoked, drained or degraded host may no longer mutate its checkpoint.
    #[must_use]
    pub const fn can_continue(self) -> bool {
        self.continuations_allowed
    }

    /// The placement choice's revision, which the insert advances.
    #[must_use]
    pub const fn run_version(self) -> i64 {
        self.run_version
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
    work_tenant: Option<&Uuid7>,
) -> Result<Option<Held>> {
    held(transaction, guard, work_tenant, sql::LOCK_HOST).await
}

/// Hold the connection and credential proof while independent fleets use the host.
/// Exclusive placement and runner-row changes wait until this transaction ends.
///
/// # Errors
/// Preserves a datastore failure while reading or locking the host.
pub async fn share_host<'a>(
    transaction: &mut Transaction<'_, Postgres>,
    guard: impl Into<Guard<'a>>,
) -> Result<Option<Held>> {
    held(transaction, guard, None, sql::SHARE_HOST).await
}

async fn held<'a>(
    transaction: &mut Transaction<'_, Postgres>,
    guard: impl Into<Guard<'a>>,
    work_tenant: Option<&Uuid7>,
    statement: &'static str,
) -> Result<Option<Held>> {
    Ok(locked(transaction, guard, work_tenant, statement)
        .await?
        .map(|(_, host)| Held {
            claims_allowed: !host.degraded
                && host.cordon_reason.is_none()
                && host.status == crate::sql::RUNNER_STATUS_ACTIVE,
            continuations_allowed: !host.degraded && renewable_state(&host.status),
            run_version: host.run_version,
            decision_selected: host.decision_selected,
            ownership: host.ownership,
        }))
}

/// The authorized host row, under its lock, and the runner it names.
async fn locked<'g>(
    transaction: &mut Transaction<'_, Postgres>,
    guard: impl Into<Guard<'g>>,
    work_tenant: Option<&Uuid7>,
    statement: &'static str,
) -> Result<Option<(Uuid7, Host)>> {
    let guard = guard.into();
    // Named from the guard's kind, never read off a missing digest.
    let connected = matches!(guard, Guard::Connected { .. });
    let (runner_id, digest, connection) = proof(guard);
    let host = sqlx::query_as::<_, Host>(statement)
        .bind(runner_id)
        .bind(digest)
        .bind(connection)
        .bind(work_tenant)
        .bind(connected)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(host
        .filter(|host| host.authorized)
        .map(|host| (runner_id.clone(), host)))
}

/// Lock a host and count live work and applicable holds using a fresh snapshot.
///
/// `None` means the authenticated host was removed. Cordoned/draining hosts
/// may finish current work but cannot claim more; revoked/degraded hosts do
/// neither. Run-specific original-expiry/fence checks remain mandatory.
///
/// # Errors
/// Preserves statement and column failures with their query context.
pub async fn lock<'a>(
    transaction: &mut Transaction<'_, Postgres>,
    guard: impl Into<Guard<'a>>,
    claimant: Claimant<'_>,
    work_tenant: Option<&Uuid7>,
) -> Result<Option<Capacity>> {
    let Some((runner_id, host)) = locked(transaction, guard, work_tenant, sql::LOCK_HOST).await?
    else {
        return Ok(None);
    };
    let census = sqlx::query_as::<_, Census>(sql::CENSUS)
        .bind(&runner_id)
        .bind(crate::sql::RUN_STATUS_ACTIVE)
        .bind(match claimant {
            Claimant::Fleet(fleet) => Some(fleet),
            Claimant::Authoring | Claimant::Existing => None,
        })
        .bind(!matches!(claimant, Claimant::Existing))
        .fetch_one(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(Some(Capacity {
        now: census.checked_at,
        workers: host.worker_count,
        occupied: census.occupied,
        held: census.held,
        claims_allowed: !host.degraded
            && host.cordon_reason.is_none()
            && host.status == crate::sql::RUNNER_STATUS_ACTIVE,
        renewals_allowed: !host.degraded && renewable_state(&host.status),
        run_version: host.run_version,
        decision_selected: host.decision_selected,
        ownership: host.ownership,
    }))
}

/// The host states a renewal may run in: one list, read by the locked
/// capacity check and by a host's renewal statement alike.
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

/// Why issue refused a placement the claim should already have filtered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementRefusal {
    /// A tenant's runner was offered another tenant's work.
    ForeignTenant,
    /// A tenant's runner was offered work that would carry a platform model key.
    PlatformModel,
}

impl PlacementRefusal {
    /// The log's `reason` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForeignTenant => "foreign_tenant",
            Self::PlatformModel => "platform_model",
        }
    }
}

/// Logs a refused placement. Any occurrence means the claim's filter has a hole,
/// so it is an error with its registry code, never a debug line.
pub fn refused_placement(runner: &Uuid7, refusal: PlacementRefusal, run_type: &str) {
    let runner_id = runner.as_str();
    let reason = refusal.as_str();
    let error_code = af_core::error_code::RUNNER_PLACEMENT_REFUSED.as_str();
    tracing::error!(
        error_code,
        runner_id,
        reason,
        run_type,
        event = "runner_placement_refused",
        "issue refused a placement the claim should have filtered"
    );
}
