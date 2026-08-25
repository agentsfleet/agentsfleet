//! The two money gates, in the order a refusal must not be charged in.
//!
//! Both answer `Option<Admission>`, where `None` continues the pass. That is
//! the shape the whole sequence composes on: a gate either produces the answer
//! or it does not, and nothing in here decides what a FAULT means — that is
//! [`super::posture`]'s, applied through the [`Gate`](super::posture::Gate)
//! value each one names.

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;

use crate::lease::admit::posture::{BALANCE, BUDGET};
use crate::lease::admit::{Admission, Refusal, Request};
use crate::money::Accounts;
use crate::money::budget::{self, Verdict};
use crate::sql;

/// The credit pool cannot cover this run.
///
/// `LOGGING_STANDARD.md` §3 `event` values, spelled as `service_billing.zig`
/// spells them so a dashboard built against the Zig daemon keeps matching.
const EVENT_BALANCE_EXHAUSTED: &str = "lease_balance_exhausted";

/// The fleet has reached its own ceiling.
const EVENT_BUDGET_BREACH: &str = "lease_budget_breach";

/// The tenant's credit pool against this run's floor cost.
///
/// Two reads, and BOTH are taken even when the first answers that the run
/// cannot be priced. An unpriceable model costs zero to admit against, but the
/// wallet read is still what makes an exhausted pool refuse — skipping it
/// because the estimate was zero would let a catalogue change silently disable
/// the credit gate.
///
/// A tenant with no wallet row is ADMITTED — see [`crate::money::wallet`] for
/// why an unprovisioned tenant is an operator gap rather than a refusal.
pub(super) async fn balance(
    accounts: &Accounts,
    request: &Request<'_>,
    tenant_id: &Uuid7,
) -> Option<Admission> {
    let estimate = match accounts
        .estimate(request.posture, request.provider, request.model)
        .await
    {
        Ok(estimate) => estimate,
        Err(fault) => return BALANCE.absorb(&fault),
    };
    let wallet = match accounts.wallet(tenant_id).await {
        Ok(wallet) => wallet,
        Err(fault) => return BALANCE.absorb(&fault),
    };
    if wallet.is_none_or(|held| held.balance.covers(estimate.floor())) {
        return None;
    }

    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let fleet = request.fleet_id.as_str();
    let event = request.event_id;
    let floor = estimate.floor().as_i64();
    tracing::debug!(
        event = EVENT_BALANCE_EXHAUSTED,
        fleet_id = fleet,
        agentsfleet_event_id = event,
        estimate_nanos = floor,
        "the tenant's credit pool cannot cover this run's floor cost"
    );
    Some(Admission::Refuse(Refusal::labelled(
        sql::event::label::BALANCE_EXHAUSTED,
    )))
}

/// The fleet's own ceiling against what it has already drained.
///
/// Runs AFTER the tenant's credit pool and BEFORE the receive debit: a refused
/// event must never be charged, and the two ceilings are independent — passing
/// one says nothing about the other.
pub(super) async fn fleet_budget(
    accounts: &Accounts,
    request: &Request<'_>,
    now: UnixMillis,
) -> Option<Admission> {
    let spend = match accounts
        .spend(request.workspace_id, request.fleet_id, now)
        .await
    {
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
    Some(Admission::Refuse(Refusal::labelled(
        sql::event::label::BUDGET_BREACH,
    )))
}
