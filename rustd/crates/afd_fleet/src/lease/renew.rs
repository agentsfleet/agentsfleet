//! The renew verb: a live run pushing its own kill deadline forward.
//!
//! What makes this more than a `SET lease_expires_at` is that reclaimability
//! and the kill deadline live on two different rows — see
//! [`crate::sql::renew::RENEW_AND_METER`]. Both move together under one fence,
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
//! The Zig reaches the same two postures inside `budgetRefusal` and
//! `creditsCover`, at their `catch` sites. Here they are the shape of this
//! file — each gate answers a value or an [`Error`](crate::Error), and the
//! posture is applied once, where a reader can see both halves at once.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::LEASE_TTL_MS;
use afd_wire::report::RenewRequest;
use sqlx::Row as _;

use crate::error::{Result, lease_lost, lease_max_runtime, query};
use crate::lease::pull::Plane;
use crate::lease::store::Leases;
use crate::sql;
use crate::sql::renew::RenewRow;
use afd_billing::rates::Posture;
use afd_billing::{Cumulative, Nanos};

/// Statement name, for the context a query failure carries.
const CONTEXT_LOAD: &str = "renew lease load";

/// Statement name, for the context a query failure carries.
const CONTEXT_RENEW: &str = "renew and meter";

/// The lease a renewal is about, as the row holds it.
#[derive(Debug, Clone)]
pub struct Renewing {
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
/// A three-variant enum, matching `renewal.zig`'s tagged union — the one place
/// the Zig already had the right shape, because it needed to carry the cap on
/// one arm and nothing on another. Kept, with the payload typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renewed {
    /// Both rows advanced. Carries the new deadline and what the slice drained.
    Extended {
        /// The instant both rows now expire at.
        expires_at: UnixMillis,
        /// Nanocredits this slice actually took.
        charged: Nanos,
    },
    /// Still the live holder, but the hard runtime ceiling is reached.
    MaxRuntime,
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
        lease_id: &str,
        runner_id: &Uuid7,
    ) -> Result<Option<Renewing>> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(sql::renew::SELECT_LEASE_FOR_RENEW)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_LOAD))?;

        let Some(row) = found else {
            return Ok(None);
        };
        let text = |index: usize| -> Result<String> {
            row.try_get::<String, _>(index).map_err(query(CONTEXT_LOAD))
        };
        let id = |index: usize, column: &'static str| -> Result<Uuid7> {
            Uuid7::parse(&text(index)?)
                .map_err(crate::error::row_malformed("fleet.runner_leases", column))
        };
        Ok(Some(Renewing {
            tenant_id: id(0, "tenant_id")?,
            fleet_id: id(1, "fleet_id")?,
            workspace_id: id(2, "workspace_id")?,
            posture: text(3)?,
            provider: text(4)?,
            model: text(5)?,
            status: text(6)?,
        }))
    }

    /// Advance both deadline rows and meter the slice, atomically.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the ledger row's
    /// identifier, an instant that cannot be encoded, and a datastore that
    /// would not answer. Every VERDICT — extended, capped, lost — is an `Ok`.
    pub async fn extend(
        &self,
        lease_id: &str,
        runner_id: &Uuid7,
        meter: afd_billing::Meter,
        now: UnixMillis,
    ) -> Result<Renewed> {
        let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let ledger_id = Uuid7::encode(now, bytes)?;

        let renew = RenewRow {
            lease_id,
            runner_id,
            now,
            want_until: now.saturating_add_millis(LEASE_TTL_MS),
            meter,
            ledger_id: &ledger_id,
        };
        let mut connection = self.pool().acquire().await?;
        let row = renew
            .bind()
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_RENEW))?;

        let column = |index: usize| -> Result<Option<i64>> {
            row.try_get::<Option<i64>, _>(index)
                .map_err(query(CONTEXT_RENEW))
        };
        let probe_found: i64 = row.try_get(0).map_err(query(CONTEXT_RENEW))?;
        let aff_updated: i64 = row.try_get(3).map_err(query(CONTEXT_RENEW))?;
        Ok(verdict_of(
            probe_found,
            column(1)?,
            column(2)?,
            aff_updated,
            column(4)?,
            now,
        ))
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
fn verdict_of(
    probe_found: i64,
    new_until: Option<i64>,
    hard_cap: Option<i64>,
    aff_updated: i64,
    charged: Option<i64>,
    now: UnixMillis,
) -> Renewed {
    if let Some(until) = new_until {
        if aff_updated != 1 {
            return Renewed::Lost;
        }
        return Renewed::Extended {
            expires_at: UnixMillis::from_millis(until),
            // A surviving guard row always prices a charge, so a null would
            // mean the guard passed without `calc` — read as zero drain rather
            // than a debit the wallet never took.
            charged: Nanos::from_i64(charged.unwrap_or(0)),
        };
    }
    if probe_found == 0 {
        return Renewed::Lost;
    }
    match hard_cap {
        Some(cap) if cap <= now.as_millis() => Renewed::MaxRuntime,
        _stale_fence => Renewed::Lost,
    }
}

impl Plane {
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
        lease_id: &str,
        request: RenewRequest,
        now: UnixMillis,
    ) -> Result<(UnixMillis, Nanos)> {
        let Some(lease) = self.leases.load_for_renew(lease_id, runner_id).await? else {
            return Err(crate::error::lease_not_found());
        };
        if lease.status != sql::LEASE_STATUS_ACTIVE {
            return Err(lease_lost());
        }
        self.gate_renewal(&lease, lease_id, now).await?;

        let cumulative = Cumulative::reported(
            request.input_tokens,
            request.cached_input_tokens,
            request.output_tokens,
        );
        let meter = match self
            .accounts
            .meter(
                posture_of(&lease),
                &lease.provider,
                &lease.model,
                cumulative,
            )
            .await
        {
            Ok(meter) => meter,
            // Fail open, for the reason the report's twin does: a catalogue
            // that will not answer must not kill a run in flight.
            Err(_unverified) => self.accounts.run_fee_meter(cumulative),
        };
        match self.leases.extend(lease_id, runner_id, meter, now).await? {
            Renewed::Extended {
                expires_at,
                charged,
            } => Ok((expires_at, charged)),
            Renewed::MaxRuntime => Err(lease_max_runtime()),
            Renewed::Lost => Err(lease_lost()),
        }
    }
}

/// The posture this lease was issued under; see the report's twin.
pub(super) fn posture_of(lease: &Renewing) -> Posture {
    Posture::parse(&lease.posture).unwrap_or(Posture::Platform)
}
