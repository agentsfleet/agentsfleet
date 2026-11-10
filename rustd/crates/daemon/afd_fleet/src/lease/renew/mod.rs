//! The renew verb: a live run pushing its own kill deadline forward.
//!
//! What makes this more than a `SET lease_expires_at` is that reclaimability
//! and the kill deadline live on two different rows — see
//! [`crate::lease::sql::meter::METER_LEASE`]. Both move together under one fence,
//! or the runner is told the lease is gone.
//!
//! # Three gates, three postures, all stated here
//!
//! The tenant's balance and the fleet's ceiling both gate a renewal, and they
//! fail in opposite directions on purpose:
//!
//! A datastore fault reading either one ADMITS the renewal. A metering outage
//! must not kill every run in flight, and a run admitted for one more slice is
//! recoverable where a killed agent is not.
//!
//! A stored budget that will not PARSE refuses. A ceiling this daemon cannot
//! read is not a ceiling it may ignore, and unlike an outage it will not clear
//! on its own.
//!
//! The two postures are the shape of this file: each gate answers a value or
//! an [`Error`](crate::Error), and the posture is applied once, where a reader
//! can see both halves at once.

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use jiff::Timestamp;

use crate::error::{
    Result, lease_cancelled, lease_lost, lease_max_runtime, lease_not_found, query,
};
use crate::lease::pull::LeaseService;
use crate::lease::sql;
use crate::lease::sql::meter::Metering;
use crate::lease::store::Leases;
use crate::lease::usage::{self, Pricing};
use afd_billing::Nanos;
use afd_billing::biller::Settlement;
use afd_billing::rates::Posture;

/// Statement name, for the context a query failure carries.
const CONTEXT_LOAD: &str = "renew lease load";

mod extend;

/// The lease a renewal is about, as the row holds it.
#[derive(Debug, Clone)]
pub struct Renewing {
    /// A committed Kill survives notification loss and refuses the next renewal.
    pub cancelled: bool,
    /// The tenant whose balance gates the renewal.
    pub tenant_id: Uuid7,
    /// The fleet whose own ceiling gates it too.
    pub fleet_id: Uuid7,
    /// The workspace the ceiling is scoped in.
    pub workspace_id: Uuid7,
    /// The billing posture resolved at issue.
    pub posture: String,
    /// The provider resolved at issue.
    pub provider: String,
    /// The model resolved at issue.
    pub model: String,
    /// Whether the lease is still `active`.
    pub status: String,
}

/// What the extend-and-meter statement decided.
///
/// Only an extended lease carries a new deadline. Refused authority may still
/// carry known usage that must settle without granting another execution slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renewed {
    /// A committed Kill forbids another execution slice.
    Cancelled,
    /// Both rows advanced. Carries the new deadline and the slice's usage line,
    /// logged in the same transaction and still to be settled; `None` when the
    /// slice measured nothing.
    Extended {
        /// The instant both rows now expire at.
        expires_at: Timestamp,
        /// The slice's priced line, as its biller charges it.
        settlement: Option<Box<Settlement>>,
    },
    /// Still the live holder, but the hard runtime ceiling is reached.
    MaxRuntime,
    /// Accounting committed, but task authority stopped during this measurement.
    Stopped {
        /// Known usage still settles through the original biller.
        settlement: Option<Box<Settlement>>,
    },
    /// The lease is no longer this runner's, or only half the extend applied.
    Lost,
}

impl Leases {
    /// The lease `lease_id` names, if it belongs to `runner_id`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row whose identifier
    /// columns are not identifiers. A transient fault here must NOT become a
    /// 404 — that would kill a healthy long-running child over a pool blip,
    /// which is why this propagates rather than answering `None`.
    pub async fn load_for_renew(
        &self,
        lease_id: &Uuid7,
        runner_id: &Uuid7,
    ) -> Result<Option<Renewing>> {
        let mut connection = self.pool().acquire().await?;
        Self::load_for_renew_on(&mut connection, lease_id, runner_id).await
    }

    pub(crate) async fn load_for_renew_on(
        connection: &mut sqlx::PgConnection,
        lease_id: &Uuid7,
        runner_id: &Uuid7,
    ) -> Result<Option<Renewing>> {
        let found: Option<LeaseForRenew> = sqlx::query_as(sql::renew::SELECT_LEASE_FOR_RENEW)
            .bind(lease_id)
            .bind(runner_id)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT_LOAD))?;
        Ok(found.map(Renewing::from))
    }
}

/// Translate the statement's five columns into the verdict.
///
/// `mapOutcome`'s logic, and the ordering of its tests is load-bearing:
///
/// A lease row that advanced while the affinity row did NOT is `Lost`, not
/// extended. A concurrent reclaim took the slot between the snapshot and the
/// update's recheck, so the deadline this would report is one the slot will not
/// honour — the child dies cleanly rather than running past a reclaim.
///
/// With no row advanced, an absent probe means the lease is not ours or not
/// active. A probe that IS there means the guard failed, on the cap or on the
/// fence; only the cap is a deterministic, reportable fact, so a stale fence
/// falls through to `Lost` alongside it.
/// The columns of [`sql::meter::METER_LEASE`]'s answer a renewal reads.
#[derive(sqlx::FromRow)]
struct RenewAnswer {
    probe_found: i64,
    new_until: Option<jiff_sqlx::Timestamp>,
    hard_cap: Option<jiff_sqlx::Timestamp>,
}

impl RenewAnswer {
    /// The new deadline, when both rows advanced.
    fn extended(&self) -> Option<Timestamp> {
        self.new_until.map(jiff_sqlx::Timestamp::to_jiff)
    }

    /// The outcome of a renewal that did NOT advance, judged against
    /// `checked_at`, the database's own instant read under the host lock.
    fn verdict(&self, checked_at: Timestamp) -> Renewed {
        if self.probe_found == 0 {
            return Renewed::Lost;
        }
        match self.hard_cap {
            Some(cap) if cap.to_jiff() <= checked_at => Renewed::MaxRuntime,
            _stale_fence => Renewed::Lost,
        }
    }
}

/// [`sql::renew::SELECT_LEASE_FOR_RENEW`]'s row, retaining native identifiers.
#[derive(sqlx::FromRow)]
struct LeaseForRenew {
    cancelled: bool,
    tenant_id: Uuid7,
    fleet_id: Uuid7,
    workspace_id: Uuid7,
    posture: String,
    provider: String,
    model: String,
    status: String,
}

impl From<LeaseForRenew> for Renewing {
    fn from(row: LeaseForRenew) -> Self {
        Self {
            cancelled: row.cancelled,
            tenant_id: row.tenant_id,
            fleet_id: row.fleet_id,
            workspace_id: row.workspace_id,
            posture: row.posture,
            provider: row.provider,
            model: row.model,
            status: row.status,
        }
    }
}

impl LeaseService {
    /// Extend one live lease, metering the slice since its last renewal.
    ///
    /// Answers the new deadline and what the slice drained.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's, one that is no longer active,
    /// one past the runtime ceiling, and one whose tenant or fleet has run out
    /// of money — each with its own registry code, because the runner acts
    /// differently on every one. Also reports a datastore that would not
    /// answer, which the runner retries at its next renewal tick rather than
    /// treating as terminal.
    pub async fn renew(
        &self,
        runner_id: &Uuid7,
        lease_id: &Uuid7,
        request: RenewRequest,
        now: Timestamp,
    ) -> Result<(Timestamp, Nanos)> {
        let Some(lease) = self.leases.load_for_renew(lease_id, runner_id).await? else {
            return Err(lease_not_found());
        };
        let lease_key = lease_id;
        let admission = if lease.cancelled {
            Err(lease_cancelled())
        } else if lease.status == sql::LEASE_STATUS_ACTIVE {
            self.gate_renewal(&lease, lease_key, lease_id.as_str(), now)
                .await
        } else {
            Err(lease_lost())
        };
        let pricing = Pricing::load(
            self.leases.pool(),
            posture_of(&lease),
            &lease.provider,
            &lease.model,
            lease_id.as_str(),
        )
        .await;
        if let Err(refusal) = admission {
            let _known = self
                .leases
                .record_known_requests(runner_id, lease_key, &pricing, &request.requests.0, now)
                .await?;
            return Err(refusal);
        }
        let metering = Metering {
            lease_id,
            runner_id,
            requests: &request.requests.0,
        };
        match self
            .leases
            .extend(metering, lease_key, &pricing, now)
            .await?
        {
            Renewed::Extended {
                expires_at,
                settlement,
            } => Ok((expires_at, usage::charged(settlement.as_deref()))),
            Renewed::Cancelled => Err(lease_cancelled()),
            Renewed::Stopped { .. } | Renewed::Lost => Err(lease_lost()),
            Renewed::MaxRuntime => Err(lease_max_runtime()),
        }
    }
}

/// The posture this lease was issued under; see the report's twin.
pub(super) fn posture_of(lease: &Renewing) -> Posture {
    Posture::parse(&lease.posture).unwrap_or(Posture::Platform)
}
