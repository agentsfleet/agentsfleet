//! What one runner's work records, inside the bound the slot table decided.
//!
//! Every function here takes the RAW identifier and lets the table decide what
//! it is attributed to. That is the cardinality guarantee expressed as an API:
//! `runner_id` is caller-supplied, an unbounded set of values would grow the
//! series set until the process died, and a producer that could pass a label
//! of its own choosing would make [`crate::runner::MAX_SERIES`] a suggestion.

use af_wire::report::FailureClass;

pub mod stream;
use opentelemetry::KeyValue;

use crate::producers::fleet::UNMODELLED_REASON;
use crate::producers::installed;
#[cfg(test)]
mod tests;

use crate::metrics::label::fleet::RunnerRun;
use crate::semconv;

/// Counts a committed automatic change with a closed, two-value label set.
pub fn cordon_changed(change: crate::metrics::label::fleet::CordonChange) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .cordon_changes
            .add(1, &[KeyValue::new(semconv::LABEL_CHANGE, change.as_str())]);
    }
}

/// Records why one runner's run failed.
///
/// How many runs ended, and how, is `agentsfleet_runner_runs_total`; this
/// family carries the reason, which that one does not.
pub fn failed(runner_id: &str, reason: Option<FailureClass>) {
    let Some(producers) = installed() else {
        return;
    };
    let label = producers.fleet.runners.admit(runner_id);
    let reason = reason.map_or(UNMODELLED_REASON, class_label);
    producers.fleet.runner_failures.add(
        1,
        &[
            KeyValue::new(semconv::LABEL_RUNNER_ID, label.to_owned()),
            KeyValue::new(semconv::LABEL_REASON, reason),
        ],
    );
    if label == crate::runner::OVERFLOW_RUNNER {
        producers.fleet.runner_failures_overflow.add(1, &[]);
    }
}

/// Records `count` runs on one runner under `outcome`: issued to it, or ended.
///
/// A count rather than one call per run, because the retention sweep expires
/// a runner's abandoned runs a batch at a time. Called after the transaction
/// that made the change commits, so a rolled-back one counts nothing.
pub fn runs(runner_id: &str, outcome: RunnerRun, count: u64) {
    if let Some(producers) = installed() {
        let label = producers.fleet.runners.admit(runner_id);
        producers.fleet.runner_runs.add(
            count,
            &[
                KeyValue::new(semconv::LABEL_RUNNER_ID, label.to_owned()),
                KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }
}

/// Records that a runner was heard from, for the gauge that reports it.
///
/// Ignored for a runner the table never admitted: a merged last-seen stamp
/// would describe whichever overflowed runner spoke most recently, which is
/// worse than not answering — counters merge meaningfully and a gauge does not.
pub fn seen(runner_id: &str, at: jiff::Timestamp) {
    if let Some(producers) = installed() {
        producers.fleet.runners.seen(runner_id, at);
    }
}

/// Records that a runner took a run.
pub fn run_taken(runner_id: &str) {
    if let Some(producers) = installed() {
        producers.fleet.runners.held(runner_id);
    }
}

/// Records that a runner gave a run back.
pub fn run_released(runner_id: &str) {
    if let Some(producers) = installed() {
        producers.fleet.runners.released(runner_id);
    }
}

/// The wire spelling of a failure class.
///
/// A match rather than a derive: the label is a wire fact and the enum's
/// variant order is not a contract. The spellings are `af_wire`'s own
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
        FailureClass::RunExpired => "run_expired",
        FailureClass::RenewalTerminate => "renewal_terminate",
        FailureClass::BudgetBreach => "budget_breach",
        FailureClass::FrameTooLarge => "frame_too_large",
    }
}
