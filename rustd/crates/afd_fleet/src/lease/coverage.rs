//! Whether a run in flight may buy another slice: the tenant's pool, and the
//! fleet's own ceiling.
//!
//! The same two gates the lease verb applies at issue, asked again mid-run —
//! and asked again on purpose. A run admitted an hour ago may have drained the
//! wallet it was admitted against, and a fleet's author may have lowered its
//! ceiling since. Both are read LIVE rather than pinned on the lease, so
//! lowering a runaway fleet's budget bites at its next renewal tick instead of
//! only at its next run.
//!
//! # Two faults, two directions, and the reason they differ
//!
//! A gate that cannot be READ admits. A metering outage that killed every run
//! in flight would turn a billing incident into a platform incident, and a run
//! given one more thirty-second slice is recoverable where a terminated agent
//! is not.
//!
//! A ceiling that cannot be PARSED refuses. It will not fix itself on the next
//! tick the way an outage will, and a limit this daemon cannot read is not one
//! it may treat as absent — that reading is what a fleet author would least
//! expect from a document they wrote to bound their spend.
//!
//! [`crate::Error::is_config_permanent`] is what separates the two, so the
//! distinction is the error's own rather than a `match` re-derived here.

use afd_core::clock::UnixMillis;

use crate::error::{Result, budget_exhausted, renewal_no_credits};
use crate::lease::pull::Plane;
use crate::lease::renew::Renewing;
use afd_billing::budget::{self, Verdict};

/// The tenant's pool cannot fund another slice.
const EVENT_NO_CREDITS: &str = "renew_no_credits";

/// The fleet reached a ceiling its author declared.
const EVENT_BUDGET_BREACH: &str = "renew_budget_breach";

/// A gate could not be read, and the renewal was admitted anyway.
const EVENT_GATE_UNREADABLE: &str = "renew_gate_unreadable_admitted";

impl Plane {
    /// Both money gates, in the order a refusal must not be charged in.
    ///
    /// The tenant's pool first, because it is the cheaper read and the more
    /// common refusal; the fleet's ceiling second. Both must pass — they are
    /// independent pools, and passing one says nothing about the other.
    ///
    /// # Errors
    /// Refuses with [`renewal_no_credits`](crate::error) when the tenant's
    /// balance is spent, and [`budget_exhausted`](crate::error) when the
    /// fleet's own ceiling is reached or its stored ceiling will not parse.
    pub(super) async fn gate_renewal(
        &self,
        lease: &Renewing,
        lease_id: &str,
        now: UnixMillis,
    ) -> Result<()> {
        self.credits_cover(lease, lease_id).await?;
        self.budget_covers(lease, lease_id, now).await
    }

    /// The tenant's credit pool against this run's floor cost.
    ///
    /// The same estimate the lease path admits against, so issue and renewal
    /// share one credit policy rather than each deciding what "enough" means.
    /// A tenant with no wallet row is admitted, for the reason the issue gate
    /// admits one.
    async fn credits_cover(&self, lease: &Renewing, lease_id: &str) -> Result<()> {
        let posture = super::renew::posture_of(lease);
        let covered = async {
            let estimate = self
                .accounts
                .estimate(posture, &lease.provider, &lease.model)
                .await?;
            let wallet = self.accounts.wallet(&lease.tenant_id).await?;
            Ok(wallet.is_none_or(|held| held.balance.covers(estimate.floor())))
        }
        .await;

        match covered {
            Ok(true) => Ok(()),
            Ok(false) => {
                let fleet = lease.fleet_id.as_str();
                tracing::warn!(
                    fleet_id = fleet,
                    lease_id,
                    event = EVENT_NO_CREDITS,
                    "the tenant's balance can no longer fund this run; not renewed"
                );
                Err(renewal_no_credits())
            }
            // Fail OPEN: the balance could not be read at all, which says
            // nothing about whether it covers the run.
            Err(fault) => {
                admit_unreadable("balance", lease, lease_id, &fault);
                Ok(())
            }
        }
    }

    /// The fleet's own ceiling against what it has already drained.
    ///
    /// Read live from `core.fleets.config_json` through the same parser that
    /// accepted the document at ingest, so the ceiling that admits a run and
    /// the ceiling that stops one can never be read two ways.
    async fn budget_covers(&self, lease: &Renewing, lease_id: &str, now: UnixMillis) -> Result<()> {
        let installed = match self.leases.installed(&lease.fleet_id).await {
            Ok(Some(installed)) => installed,
            // A fleet an operator stopped mid-run has no ceiling left to
            // enforce and no author waiting on one. The run finishes; stopping
            // a fleet has never killed work already in flight.
            Ok(None) => return Ok(()),
            // The one CLOSED arm. A stored ceiling this daemon cannot parse is
            // not a ceiling it may ignore — and unlike the outage below, it
            // will read the same on every subsequent tick.
            Err(fault) if fault.is_config_permanent() => {
                let fleet = lease.fleet_id.as_str();
                let reason = fault.to_string();
                tracing::warn!(
                    fleet_id = fleet,
                    lease_id,
                    reason,
                    event = EVENT_BUDGET_BREACH,
                    "the fleet's stored budget cannot be read; not renewed"
                );
                return Err(budget_exhausted());
            }
            Err(fault) => {
                admit_unreadable("budget", lease, lease_id, &fault);
                return Ok(());
            }
        };

        let spend = match self
            .accounts
            .spend(&lease.workspace_id, &lease.fleet_id, now)
            .await
        {
            Ok(spend) => spend,
            // Lifted into this crate's error first, so all three gates log
            // through one shape and the operator greps one event.
            Err(fault) => {
                admit_unreadable("spend", lease, lease_id, &crate::Error::from(fault));
                return Ok(());
            }
        };
        let verdict = budget::covers(installed.config.budget(), spend);
        if verdict == Verdict::Admit {
            return Ok(());
        }

        let fleet = lease.fleet_id.as_str();
        let which = verdict.as_str();
        tracing::warn!(
            fleet_id = fleet,
            lease_id,
            verdict = which,
            event = EVENT_BUDGET_BREACH,
            "the fleet reached a ceiling its author declared; not renewed"
        );
        Err(budget_exhausted())
    }
}

/// Admit a renewal whose gate could not be read, and say so.
///
/// One place, so the fail-open posture is applied identically at all three call
/// sites and an operator greps one event for every instance of it. Logs and
/// nothing else — each call site states its own `Ok(())` beside it, so the
/// admission is visible AT the arm that takes it rather than inferred from a
/// return type three screens further down.
fn admit_unreadable(gate: &'static str, lease: &Renewing, lease_id: &str, fault: &crate::Error) {
    let fleet = lease.fleet_id.as_str();
    let reason = fault.to_string();
    let code = fault.code().as_str();
    tracing::warn!(
        error_code = code,
        fleet_id = fleet,
        lease_id,
        gate,
        reason,
        event = EVENT_GATE_UNREADABLE,
        "a renewal gate could not be read; the run is admitted for one more slice"
    );
}
