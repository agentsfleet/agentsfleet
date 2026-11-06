//! The two money gates, in the order a refusal must not be charged in.
//!
//! Each JUDGES reads the caller already made: the estimate, the balance and
//! the fleet's spend depend on nothing but the request, so [`super::money_gates`]
//! starts all three together and hands their answers here in judging order.
//! Both answer `Option<Declined>`, where `None` continues the pass. Nothing in
//! here decides what a FAULT means — that is [`super::fault`]'s, applied
//! through the [`Gate`](super::fault::Gate) value each one names.

use afd_core::error_code;

use afd_billing::biller::Balance;
use afd_billing::rates::Estimate;

use crate::lease::admit::fault::{BALANCE, BUDGET};
use crate::lease::admit::{Declined, Refusal, Request};
use afd_billing::Spend;
use afd_billing::budget::{self, Verdict};

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
/// Two reads, and BOTH are judged even when the first answers that the run
/// cannot be priced. An unpriceable model costs zero to admit against, but the
/// balance read is still what makes an exhausted pool refuse — skipping it
/// because the estimate was zero would let a catalogue change silently disable
/// the credit gate.
///
/// A tenant with no wallet row is ADMITTED — see [`afd_billing::wallet`] for
/// why an unprovisioned tenant is an operator gap rather than a refusal.
pub(super) fn balance(
    request: &Request<'_>,
    estimate: afd_billing::Result<Estimate>,
    balance: afd_billing::Result<Balance>,
) -> Option<Declined> {
    let estimate = match estimate {
        Ok(estimate) => estimate,
        Err(fault) => return BALANCE.absorb(&fault),
    };
    let floor = estimate.floor();
    let available = match balance {
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
        event = EVENT_BALANCE_EXHAUSTED,
        fleet_id = fleet,
        agentsfleet_event_id = event,
        estimate_nanos = floor_nanos,
        "the tenant's credit pool cannot cover this run's floor cost"
    );
    Some(Declined::Refuse(Refusal::labelled(
        afd_core::event::label::BALANCE_EXHAUSTED,
    )))
}

/// The fleet's own ceiling against what it has already drained.
///
/// Runs AFTER the tenant's credit pool and BEFORE the receive debit: a refused
/// event must never be charged, and the two ceilings are independent — passing
/// one says nothing about the other.
pub(super) fn fleet_budget(
    request: &Request<'_>,
    spend: afd_billing::Result<Spend>,
) -> Option<Declined> {
    let spend = match spend {
        Ok(spend) => spend,
        Err(fault) => return BUDGET.absorb(&fault),
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
        event = EVENT_BUDGET_BREACH,
        fleet_id = fleet,
        agentsfleet_event_id = event,
        verdict = which,
        "the fleet has reached a ceiling its author declared"
    );
    Some(Declined::Refuse(Refusal::labelled(
        afd_core::event::label::BUDGET_BREACH,
    )))
}
