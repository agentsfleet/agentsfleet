//! The money gate: the payer's balance against this run's floor cost.
//!
//! It answers `Option<Declined>`, where `None` continues the pass. Nothing in
//! here decides what a FAULT means — that is [`super::fault`]'s, applied
//! through the [`Gate`](super::fault::Gate) value it names.

use af_core::error_code;
use af_core::id::Uuid7;

use afd_billing::biller::{Account, Biller};
use afd_billing::rates::Estimate;

use crate::run::start_check::fault::BALANCE;
use crate::run::start_check::{Declined, Refusal, Request};

/// The credit pool cannot cover this run.
///
/// LOG-EVENT (`.orly/standards/domains/logging.md`) values. Dashboards match on these
/// spellings, so changing one is an operator-visible change.
const EVENT_BALANCE_EXHAUSTED: &str = "run_balance_exhausted";

/// The tenant's credit pool against this run's floor cost.
///
/// Answers a refusal when the pool cannot cover the run's floor. The pool is
/// the biller's balance, which every line the tenant has logged already
/// debited, so what runs already in flight spent counts against it.
///
/// Two reads, and BOTH are taken even when the first answers that the run
/// cannot be priced. An unpriceable model costs zero to start against, but the
/// balance read is still what makes an exhausted pool refuse — skipping it
/// because the estimate was zero would let a catalogue change silently disable
/// the credit gate.
///
/// A tenant with no wallet row is refused as an exhausted one: the payer's
/// balance is the only ceiling a run has, and a payer with none cannot fund it.
pub(super) async fn balance<F: error_code::Coded + Sync>(
    estimate: Result<Estimate, &F>,
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
        Ok(funds) => funds.balance,
        Err(fault) if fault.is_wallet_missing() => return Some(exhausted()),
        Err(fault) => return BALANCE.absorb(&fault),
    };
    if available.covers(floor) {
        return None;
    }

    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let fleet = request.fleet_id.as_str();
    let event = request.turn_id;
    let floor_nanos = floor.as_i64();
    {
        let (event, fleet_id, agentsfleet_turn_id, estimate_nanos, message) = (
            &EVENT_BALANCE_EXHAUSTED,
            &fleet,
            &event,
            &floor_nanos,
            format_args!("the tenant's credit pool cannot cover this run's floor cost"),
        );
        tracing::debug!(message, %event, %fleet_id, %agentsfleet_turn_id, estimate_nanos);
    };
    Some(exhausted())
}

/// The refusal an unfunded payer earns.
const fn exhausted() -> Declined {
    Declined::Refuse(Refusal::labelled(af_core::turn::label::BALANCE_EXHAUSTED))
}
