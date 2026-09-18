//! What one runner's work records, inside the bound the slot table decided.
//!
//! Every function here takes the RAW identifier and lets the table decide what
//! it is attributed to. That is the cardinality guarantee expressed as an API:
//! `runner_id` is caller-supplied, an unbounded set of values would grow the
//! series set until the process died, and a producer that could pass a label
//! of its own choosing would make [`crate::runner::MAX_SERIES`] a suggestion.

use afd_wire::report::{FailureClass, Outcome};
use opentelemetry::KeyValue;

use crate::producers::fleet::UNMODELLED_REASON;
use crate::producers::{Producers, installed};
#[cfg(test)]
mod tests;

use crate::metrics::label::fleet::Fault;
use crate::semconv;

/// Records one runner's failed run.
///
/// Two families move, because a failed run is both a failure and a finished
/// run. Recording only the failure would make the execution total disagree
/// with the sum of its outcomes, and an operator would be looking for the
/// difference rather than for the failure.
pub fn failed(runner_id: &str, reason: Option<FailureClass>) {
    let Some(producers) = installed() else {
        return;
    };
    let label = producers.fleet.runners.admit(runner_id);
    // Classified before `reason` is shadowed by its wire spelling: the blame
    // split reads the typed class, not the string it renders to.
    let fault = fault_of(reason);
    let reason = reason.map_or(UNMODELLED_REASON, class_label);
    producers.fleet.runner_failures.add(
        1,
        &[
            KeyValue::new(semconv::LABEL_RUNNER_ID, label.to_owned()),
            KeyValue::new(semconv::LABEL_REASON, reason),
        ],
    );
    executed(producers, label, Outcome::FleetError, Some(fault));
    if label == crate::runner::OVERFLOW_RUNNER {
        producers.fleet.runner_failures_overflow.add(1, &[]);
    }
}

/// Records one runner's finished run.
pub fn processed(runner_id: &str) {
    if let Some(producers) = installed() {
        let label = producers.fleet.runners.admit(runner_id);
        executed(producers, label, Outcome::Processed, None);
    }
}

/// Records that a runner was heard from, for the gauge that reports it.
///
/// Ignored for a runner the table never admitted: a merged last-seen stamp
/// would describe whichever overflowed runner spoke most recently, which is
/// worse than not answering — counters merge meaningfully and a gauge does not.
pub fn seen(runner_id: &str, at_ms: i64) {
    if let Some(producers) = installed() {
        producers.fleet.runners.seen(runner_id, at_ms);
    }
}

/// Records that a runner took a lease.
pub fn lease_taken(runner_id: &str) {
    if let Some(producers) = installed() {
        producers.fleet.runners.leased(runner_id);
    }
}

/// Records that a runner gave a lease back.
pub fn lease_released(runner_id: &str) {
    if let Some(producers) = installed() {
        producers.fleet.runners.released(runner_id);
    }
}

/// Which side of the platform boundary a failure falls on.
///
/// A match with no default arm, deliberately: a twelfth [`FailureClass`] must
/// not compile until somebody decides whose fault it is. A default would pick
/// a side silently, and the side it picked would be wrong half the time.
///
/// An unclassified failure is the PLATFORM's. The runner reports `None` when it
/// could not say why a run died, and a cause nobody could name is not evidence
/// against the tenant — failing toward our own accountability is the only
/// direction that cannot quietly flatter the objective.
const fn fault_of(reason: Option<FailureClass>) -> Fault {
    match reason {
        Some(
            FailureClass::PolicyDeny
            | FailureClass::LandlockDeny
            | FailureClass::OomKill
            | FailureClass::ResourceKill
            | FailureClass::BudgetBreach
            | FailureClass::TimeoutKill,
        ) => Fault::Workload,
        Some(
            FailureClass::StartupPosture
            | FailureClass::RunnerCrash
            | FailureClass::TransportLoss
            | FailureClass::LeaseExpired
            | FailureClass::RenewalTerminate,
        )
        | None => Fault::Platform,
    }
}

/// Records one finished run under an already-decided label.
///
/// A success carries no fault label at all rather than a third "none" value, so
/// `{fault!="workload"}` reads as "everything the platform was accountable
/// for" — an absent label satisfies a not-equal matcher.
fn executed(producers: &Producers, label: &str, outcome: Outcome, fault: Option<Fault>) {
    let mut labels = vec![
        KeyValue::new(semconv::LABEL_RUNNER_ID, label.to_owned()),
        KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()),
    ];
    if let Some(fault) = fault {
        labels.push(KeyValue::new(semconv::LABEL_FAULT, fault.as_str()));
    }
    producers.fleet.runner_executions.add(1, &labels);
}

/// The wire spelling of a failure class.
///
/// A match rather than a derive: the label is a wire fact and the enum's
/// variant order is not a contract. The spellings are `afd_wire`'s own
/// `snake_case` serde renames, so a class reads the same on a metric label as
/// it does in the report that carried it.
const fn class_label(class: FailureClass) -> &'static str {
    match class {
        FailureClass::StartupPosture => "startup_posture",
        FailureClass::PolicyDeny => "policy_deny",
        FailureClass::TimeoutKill => "timeout_kill",
        FailureClass::OomKill => "oom_kill",
        FailureClass::ResourceKill => "resource_kill",
        FailureClass::RunnerCrash => "runner_crash",
        FailureClass::TransportLoss => "transport_loss",
        FailureClass::LandlockDeny => "landlock_deny",
        FailureClass::LeaseExpired => "lease_expired",
        FailureClass::RenewalTerminate => "renewal_terminate",
        FailureClass::BudgetBreach => "budget_breach",
    }
}
