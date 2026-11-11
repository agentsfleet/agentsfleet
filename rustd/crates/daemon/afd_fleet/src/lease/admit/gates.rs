//! The two money gates, in the order a refusal must not be charged in.
//!
//! Both answer `Option<Declined>`, where `None` continues the pass. That is
//! the shape the whole sequence composes on: a gate either produces the answer
//! or it does not, and nothing in here decides what a FAULT means — that is
//! [`super::fault`]'s, applied through the [`Gate`](super::fault::Gate)
//! value each one names.

use af_core::error_code;
use af_core::id::Uuid7;

use afd_billing::biller::{Account, Biller};

use crate::lease::admit::fault::{BALANCE, BUDGET};
use crate::lease::admit::{Declined, Refusal, Request};
use afd_billing::Nanos;
use afd_billing::budget::{self, Spend, Verdict};
use afd_billing::rates::Estimate;
use afd_gate::gate::Recorded;
use afd_gate::policy::grants::Grants;

use crate::error::query;
use crate::lease::envelope::Acquired;
use crate::lease::sql::gates::SELECT_GATES;
use crate::lease::store::Leases;

/// The gates read, for a query failure's context.
const CONTEXT_GATES: &str = "lease gates read";

/// Every fact the gates judge before an issue, or the fault the one read met.
#[derive(Debug)]
pub(crate) enum GateFacts {
    /// The read answered.
    Read {
        /// The fleet's spend inside its budget windows.
        spend: Spend,
        /// The gate this event already holds, evaluated.
        recorded: Recorded,
        /// The integrations the fleet holds a standing grant for.
        granted: Grants,
    },
    /// The read failed: each gate applies the posture it applies to its own
    /// read failing, so the outcome matches every read failing alone.
    Unreadable(crate::Error),
}

impl GateFacts {
    /// The spend, or the fault the budget gate absorbs.
    pub(crate) fn spend(&self) -> core::result::Result<Spend, &crate::Error> {
        match self {
            Self::Read { spend, .. } => Ok(*spend),
            Self::Unreadable(fault) => Err(fault),
        }
    }

    /// The recorded gate as the approval gate judges it.
    pub(crate) fn recorded(&self) -> Recorded {
        match self {
            Self::Read { recorded, .. } => recorded.clone(),
            Self::Unreadable(fault) => {
                Recorded::Unreadable(af_core::error_code::Coded::logged(fault))
            }
        }
    }

    /// The standing grants, when the read answered.
    pub(crate) fn granted(self) -> Option<Grants> {
        match self {
            Self::Read { granted, .. } => Some(granted),
            Self::Unreadable(_) => None,
        }
    }
}

/// [`SELECT_GATES`]'s row.
#[derive(Debug, sqlx::FromRow)]
struct GatesRow {
    day_nanos: i64,
    month_nanos: i64,
    gate_action_id: Option<Uuid7>,
    gate_status: Option<String>,
    gate_lapsed: Option<bool>,
    approved_services: Vec<String>,
}

impl Leases {
    /// Every fact the gates judge for `acquired`, in one statement.
    pub(crate) async fn gate_facts(&self, acquired: &Acquired) -> GateFacts {
        match self.read_gates(acquired).await {
            Ok(row) => GateFacts::Read {
                spend: Spend {
                    day: Nanos::from_i64(row.day_nanos),
                    month: Nanos::from_i64(row.month_nanos),
                },
                recorded: Recorded::observed(
                    row.gate_action_id.as_ref(),
                    row.gate_status.as_deref(),
                    row.gate_lapsed,
                ),
                granted: row.approved_services.into_iter().collect(),
            },
            Err(fault) => GateFacts::Unreadable(fault),
        }
    }

    async fn read_gates(&self, acquired: &Acquired) -> crate::Result<GatesRow> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_as(SELECT_GATES)
            .bind(&acquired.workspace_id)
            .bind(&acquired.fleet_id)
            .bind(af_core::timing::DAY)
            .bind(&acquired.event_id)
            .bind(afd_gate::gate::grant_sql::STATUS_APPROVED)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_GATES))
    }
}

/// The credit pool cannot cover this run.
///
/// LOG-EVENT (`.orly/standards/domains/logging.md`) values. Dashboards match on these
/// spellings, so changing one is an operator-visible change.
const EVENT_BALANCE_EXHAUSTED: &str = "lease_balance_exhausted";

/// The fleet has reached its own ceiling.
const EVENT_BUDGET_BREACH: &str = "lease_budget_breach";

/// The tenant's credit pool against this run's floor cost.
///
/// Answers a refusal when the pool cannot cover the run's floor. The pool is
/// the biller's balance less every line the tenant has logged and the relay
/// has not yet charged, so what runs already in flight spent counts against
/// it.
///
/// Two reads, and BOTH are taken even when the first answers that the run
/// cannot be priced. An unpriceable model costs zero to admit against, but the
/// balance read is still what makes an exhausted pool refuse — skipping it
/// because the estimate was zero would let a catalogue change silently disable
/// the credit gate.
///
/// A tenant with no wallet row is ADMITTED — see [`afd_billing::wallet`] for
/// why an unprovisioned tenant is an operator gap rather than a refusal.
pub(super) async fn balance<F: af_core::error_code::Coded>(
    estimate: core::result::Result<Estimate, &F>,
    biller: &dyn Biller,
    request: &Request<'_>,
    tenant_id: &Uuid7,
) -> Option<Declined> {
    let estimate = match estimate {
        Ok(estimate) => estimate,
        Err(fault) => return BALANCE.absorb(fault),
    };
    let floor = estimate.floor();
    let payer = Account {
        tenant: tenant_id.clone(),
        workspace: request.workspace_id.clone(),
    };
    let available = match biller.balance(&payer).await {
        Ok(funds) => funds.left(),
        Err(fault) if fault.is_wallet_missing() => return None,
        Err(fault) => return BALANCE.absorb(&fault),
    };
    if available.covers(floor) {
        return None;
    }

    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let fleet = request.fleet_id.as_str();
    let event = request.event_id;
    let floor_nanos = floor.as_i64();
    tracing::debug!(
        event = %EVENT_BALANCE_EXHAUSTED,
        fleet_id = %fleet,
        agentsfleet_event_id = %event,
        estimate_nanos = floor_nanos,
        "the tenant's credit pool cannot cover this run's floor cost"
    );
    Some(Declined::Refuse(Refusal::labelled(
        af_core::event::label::BALANCE_EXHAUSTED,
    )))
}

/// The fleet's own ceiling against what it has already drained.
///
/// Runs AFTER the tenant's credit pool and BEFORE the receive debit: a refused
/// event must never be charged, and the two ceilings are independent — passing
/// one says nothing about the other.
/// The fleet budget gate over a spend the caller read, or the fault its read
/// met, which the gate absorbs under its declared posture.
pub(super) fn fleet_budget<E: af_core::error_code::Coded>(
    request: &Request<'_>,
    spend: core::result::Result<Spend, &E>,
) -> Option<Declined> {
    let spend = match spend {
        Ok(spend) => spend,
        Err(fault) => return BUDGET.absorb(fault),
    };
    let verdict = budget::covers(request.budget, spend);
    if verdict == Verdict::Admit {
        return None;
    }

    let code = error_code::RUN_BUDGET_EXCEEDED.as_str();
    let fleet = request.fleet_id.as_str();
    let event = request.event_id;
    let which = verdict.as_str();
    tracing::debug!(
        error_code = code,
        event = %EVENT_BUDGET_BREACH,
        fleet_id = %fleet,
        agentsfleet_event_id = %event,
        verdict = which,
        "the fleet has reached a ceiling its author declared"
    );
    Some(Declined::Refuse(Refusal::labelled(
        af_core::event::label::BUDGET_BREACH,
    )))
}
