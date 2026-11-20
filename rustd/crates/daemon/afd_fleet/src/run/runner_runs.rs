//! Each run counted on its runner, as `agentsfleet_runner_runs_total`, once
//! the transaction that issued, settled or expired it commits.
//!
//! The count is a metric and never a row: Postgres keeps the runs themselves,
//! and an operator reads the totals in Grafana. Counting after the commit
//! rather than inside the statement means a transaction that rolls back counts
//! nothing. The retention sweep's expiries are counted by `afd_runner`, which
//! runs that sweep.

use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::RunnerRun;
use af_observability::producers::fleet::runner;

/// A run issued to `runner_id`.
pub(super) fn acquired(runner_id: &Uuid7) {
    runner::runs(runner_id.as_str(), RunnerRun::Acquired, 1);
}

/// A run `runner_id` reported: succeeded when it finished cleanly, failed for
/// any other report, a run a person stopped included.
pub(super) fn reported(runner_id: &Uuid7, succeeded: bool) {
    let outcome = if succeeded {
        RunnerRun::Succeeded
    } else {
        RunnerRun::Failed
    };
    runner::runs(runner_id.as_str(), outcome, 1);
}

/// A lapsed run of `runner_id`'s, expired by the assignment that took its
/// work over.
pub(super) fn expired(runner_id: &Uuid7) {
    runner::runs(runner_id.as_str(), RunnerRun::Expired, 1);
}

#[cfg(test)]
#[path = "runner_runs_tests.rs"]
mod tests;
