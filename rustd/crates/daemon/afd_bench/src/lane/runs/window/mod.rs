//! One measured window: every runner polling the assignment pass at once, and
//! the cost either side of it.
//!
//! Split from the lane's orchestration at the file cap, along the seam the
//! lane already had: `run.rs` decides WHAT runs in which order, and this
//! decides how one window is driven, counted and written into the report.

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use af_core::id::Uuid7;
use afd_fleet::run::Runs;

use super::drive::{self, Shared};
use crate::abort::Abort;
use crate::datastores::Datastores;
use crate::error::Result;
use crate::instrument::{PollCounters, RunInstrument};
use crate::lane::joined;
use crate::lane::outcomes::Outcomes;
use crate::report::{DatastoreCost, DatastoreCosts, Report, count, per_second, ratio};
use crate::statements;

/// Measurement key: polls issued per second, run or miss.
const POLLS_PER_SECOND: &str = "polls_per_second";

/// Measurement key: how many runs the window issued in total.
const RUNS: &str = "runs";

/// Measurement key: polls the path refused.
const FAILURES: &str = "failures";

/// Measurement key: the fraction of everything tried that the path refused.
const ERROR_RATE: &str = "error_rate";

/// Measurement key: fleets each poll examined, read off the daemon's counter.
const CANDIDATES_PER_POLL: &str = "candidates_per_poll";

/// Measurement key: the fraction of polls that produced no work.
const WASTED_ASSIGNMENT_RATE: &str = "wasted_assignment_rate";

/// Measurement key: whether the contended window ended at exhaustion (1) or
/// at its deadline (0), so a reader knows which the rate is over.
const EXHAUSTED: &str = "exhausted";

/// One window: every runner polling at once, with the cost either side of it.
pub(super) struct Window {
    outcomes: Outcomes,
    length: Duration,
    exhausted: bool,
    counters: PollCounters,
    /// Statements Postgres executed in the window, from `pg_stat_statements`:
    /// every crate's, which a tally kept by the poll never was.
    statements: u64,
}

/// What both of a run's windows drive and read: the same runners, the same
/// run path, the same counters and the same abort monitor.
pub(super) struct Pollers<'run> {
    /// The daemon's own round-trip counters.
    pub(super) instrument: &'run RunInstrument,
    /// The run path under measurement.
    pub(super) runs: &'run Runs,
    /// The datastores, for the statement count.
    pub(super) stores: &'run Datastores,
    /// The enrolled runners, one polling task each.
    pub(super) runners: &'run [Uuid7],
    /// The monitor every outcome is reported to.
    pub(super) abort: &'run Arc<Abort>,
}

impl Pollers<'_> {
    /// Drive every runner concurrently for one window, measuring what it cost.
    pub(super) async fn measure(
        &self,
        window: Duration,
        stop_after: Option<u64>,
    ) -> Result<Window> {
        statements::install(&self.stores.database).await?;
        let statements_before = statements::read(&self.stores.database).await?;
        let before = self.instrument.read()?;
        let started = Instant::now();
        let shared = Arc::new(Shared {
            deadline: started + window,
            held: AtomicU64::new(0),
            stop_after,
            abort: Arc::clone(self.abort),
        });

        let (outcomes, last_run) = drive_all(self.runs, self.runners, &shared).await?;
        let ended = Instant::now();
        let exhausted = stop_after.is_some_and(|ceiling| shared.held_so_far() >= ceiling);

        Ok(Window {
            outcomes,
            length: drive::window_length(started, ended, last_run, exhausted),
            exhausted,
            counters: self.instrument.read()?.since(before),
            statements: statements::read(&self.stores.database)
                .await?
                .since(statements_before)
                .statements,
        })
    }
}

/// Every runner polling at once, answering what they did together and when
/// the last of them issued a run.
async fn drive_all(
    runs: &Runs,
    runners: &[Uuid7],
    shared: &Arc<Shared>,
) -> Result<(Outcomes, Option<Instant>)> {
    let mut tasks = Vec::with_capacity(runners.len());
    for runner in runners {
        let runs = runs.clone();
        let runner = runner.clone();
        let shared = Arc::clone(shared);
        // Per runner, because the thing under measurement is what happens when
        // R of them reach the same turn queue at the same instant.
        tasks.push(tokio::spawn(async move {
            drive::poll_until(&runs, &runner, &shared).await
        }));
    }

    let mut outcomes = Outcomes::new()?;
    let mut last_run: Option<Instant> = None;
    for task in tasks {
        let (theirs, their_last) = joined(task.await, "runner")??;
        outcomes.absorb(&theirs)?;
        last_run = last_run.max(their_last);
    }
    Ok((outcomes, last_run))
}

impl Window {
    /// Write the contended window's numbers into the report.
    pub(super) fn record(&self, report: &mut Report) {
        let seconds = self.length.as_secs_f64();
        report.latency(seconds, &self.outcomes.latency);
        report.measurement(
            POLLS_PER_SECOND,
            per_second(self.outcomes.attempts(), seconds),
        );
        report.measurement(RUNS, count(self.outcomes.successes));
        report.measurement(FAILURES, count(self.outcomes.failures));
        report.measurement(ERROR_RATE, self.outcomes.failure_fraction());
        report.measurement(EXHAUSTED, if self.exhausted { 1.0 } else { 0.0 });
        report.measurement(WASTED_ASSIGNMENT_RATE, self.outcomes.wasted_fraction());
        report.measurement(
            CANDIDATES_PER_POLL,
            ratio(self.counters.candidates, self.counters.polls),
        );
        // No `time_ms`: this lane times the poll end to end, which is already
        // the p95, and splitting out Postgres's share would need a timer
        // inside the pass rather than around it.
        report.datastores = DatastoreCosts {
            postgres: DatastoreCost {
                operations: self.statements,
                time_ms: None,
            },
        };
    }
}

#[cfg(test)]
mod tests;
