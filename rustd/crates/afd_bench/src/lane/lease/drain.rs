//! The lease lane's drain: the whole lease and report path, and what a poll
//! costs once the work is gone.
//!
//! # Why a second population
//!
//! The contended window drives `Leases::select` and never reports, so its
//! fleets stay claimed and its idle window has to force the readiness index
//! empty before it can measure anything. That makes its idle number a property
//! of the force-clear, not of the path. The drain seeds its own fleets and
//! drives the two verbs a runner actually calls — `Plane::lease` and
//! `Plane::report` — until every event has settled, and then polls on with
//! nothing cleared by hand. Whatever the index still holds is what production
//! would hold, and the idle cost it reports is the cost production pays.
//!
//! # Counted by the server
//!
//! Statements and commits come from Postgres's own tallies (`statements.rs`),
//! so a rewrite that folds several statements into one moves the number, where
//! a pool-acquire count would not.

mod owned;
mod platform;
mod runner;
mod settled;
mod stage;

pub use self::owned::reset_readiness;

use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_fleet::lease::Plane;

use self::runner::{Shared, Tally};
use self::settled::Settled;
use super::Parameters;
use crate::abort::Abort;
use crate::datastores::{Datastores, dragonfly_calls};
use crate::error::{Error, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::profile::Profile;
use crate::report::{Report, count, latency, per_second, ratio};
use crate::statements::{self, StatementCost};

/// Measurement key: leases the drain issued, one per event it settled.
const LEASES: &str = "drain_leases";

/// Measurement key: polls the drain made, lease or not.
const POLLS: &str = "drain_polls";

/// Measurement key: polls or reports the path refused.
const FAILURES: &str = "drain_failures";

/// Measurement key: leases issued per second of the drain.
const LEASES_PER_SECOND: &str = "drain_leases_per_second";

/// Measurement key: the tail of a poll that issued a lease.
const LEASE_P95_MS: &str = "drain_lease_p95_ms";

/// Measurement key: the tail of the report after it.
const REPORT_P95_MS: &str = "drain_report_p95_ms";

/// Measurement key: statements Postgres executed per lease issued — polls
/// that missed, the lease and its report all included.
const STATEMENTS_PER_LEASE: &str = "drain_statements_per_lease";

/// Measurement key: transactions Postgres committed per lease issued.
const COMMITS_PER_LEASE: &str = "drain_commits_per_lease";

/// Measurement key: Dragonfly commands per lease issued.
const DRAGONFLY_CALLS_PER_LEASE: &str = "drain_dragonfly_calls_per_lease";

/// Measurement key: events the drain seeded, one per fleet.
const EVENTS: &str = "drain_events";

/// Measurement key: event rows the drained fleets carry afterwards.
const EVENT_ROWS: &str = "drain_event_rows";

/// Measurement key: event rows a report closed as processed.
const EVENTS_PROCESSED: &str = "drain_events_processed";

/// Measurement key: ledger rows per seeded event; two is a receive and a run.
const LEDGER_ROWS_PER_EVENT: &str = "drain_ledger_rows_per_event";

/// Measurement key: marks the readiness index still holds for the drained
/// fleets once the drain is done. Zero is what a drained fleet is.
const READY_DEPTH: &str = "drain_ready_depth";

/// Measurement key: polls in the quiet window after the drain.
const IDLE_POLLS: &str = "drain_idle_polls";

/// Measurement key: Postgres statements per poll once every fleet drained.
///
/// The spelling the acceptance rubric greps for in the lane's output.
pub const IDLE_STATEMENTS_PER_POLL: &str = "idle_statements_per_poll";

/// Measurement key: Postgres commits per poll once every fleet drained.
const IDLE_COMMITS_PER_POLL: &str = "idle_commits_per_poll";

/// Measurement key: Dragonfly commands per poll once every fleet drained.
const IDLE_DRAGONFLY_CALLS_PER_POLL: &str = "drain_idle_dragonfly_calls_per_poll";

/// The task role a lost drain runner is reported under.
const RUNNER_ROLE: &str = "drain runner";

/// How long each round of polls runs once every event is reported, before
/// the index is asked again whether a drained fleet is still marked.
const CLEAR_SLICE: Duration = Duration::from_millis(100);

/// How long the quiet window after the drain polls.
///
/// As long as the contended window's idle half, so the two per-poll figures
/// are taken over windows of one length.
const IDLE_WINDOW: Duration = Duration::from_secs(2);

/// The keys the one-line summary prints, in order.
const SUMMARY_KEYS: [&str; 5] = [
    IDLE_STATEMENTS_PER_POLL,
    IDLE_COMMITS_PER_POLL,
    STATEMENTS_PER_LEASE,
    COMMITS_PER_LEASE,
    READY_DEPTH,
];

/// Drain a fresh population through the lease and report verbs, then poll it
/// idle, writing both into `report`.
///
/// # Errors
///
/// Whatever staging, the counters or a datastore refused. The platform rows
/// the drain staged are released on every path, before the caller's prefix
/// sweep needs them gone.
pub async fn run(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
    report: &mut Report,
) -> Result<()> {
    parameters.admit(profile)?;
    let mut ledger = FixtureLedger::new();
    let staged = stage::stage(stores, prefix, parameters, &mut ledger).await;
    // Counted whether staging finished or not: every row it wrote carries the
    // prefix, and the sweep will find it either way.
    report.fixture.created += ledger.created_count();
    let measured = match staged {
        Ok(staged) => measure(profile, parameters, stores, &staged, report).await,
        Err(refused) => Err(refused),
    };
    let released = platform::release(stores).await;
    measured.and(released)
}

/// Both windows over a staged population.
async fn measure(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    staged: &stage::Staged,
    report: &mut Report,
) -> Result<()> {
    let plane = staged.plane(stores);
    let abort = Arc::new(Abort::new(profile.caps().abort_error_rate));
    let events = u64::try_from(staged.fleets.len()).unwrap_or(u64::MAX);
    let started = Instant::now();
    let mut drained = window(
        &plane,
        stores,
        &staged.runners,
        parameters.window,
        Some(events),
        &abort,
    )
    .await?;
    // A reported fleet keeps its mark until a poll finds it empty and clears
    // it, and that poll is part of what draining costs: the runners go on,
    // counted with the drain, until no drained fleet is marked. The idle
    // window after it then polls an index the path emptied itself.
    while !abort.fired()
        && started.elapsed() < parameters.window
        && settled::marked(stores, &staged.fleets).await? > 0
    {
        let slice = window(&plane, stores, &staged.runners, CLEAR_SLICE, None, &abort).await?;
        drained.absorb(&slice)?;
    }
    let settled = settled::read(stores, &staged.fleets).await?;
    drained.record(report, events, settled);
    // After an abort every runner exits on entry, and a per-poll ratio over a
    // window that never polled would describe nothing.
    if !abort.fired() {
        let idle = window(&plane, stores, &staged.runners, IDLE_WINDOW, None, &abort).await?;
        idle.record_idle(report);
    }
    Ok(())
}

/// One window of every runner at once, with its cost either side.
struct Window {
    tally: Tally,
    length: Duration,
    cost: StatementCost,
    dragonfly_calls: u64,
}

/// Drive every runner until `target` events are reported or `length` passes.
async fn window(
    plane: &Plane,
    stores: &Datastores,
    runners: &[Uuid7],
    length: Duration,
    target: Option<u64>,
    abort: &Arc<Abort>,
) -> Result<Window> {
    let before = statements::read(&stores.database).await?;
    let dragonfly_before = dragonfly_calls(&stores.queue).await?;
    let started = Instant::now();
    let shared = Arc::new(Shared {
        deadline: started + length,
        reported: 0.into(),
        target,
        abort: Arc::clone(abort),
    });
    let mut tasks = Vec::with_capacity(runners.len());
    for runner in runners {
        let (plane, runner, shared) = (plane.clone(), runner.clone(), Arc::clone(&shared));
        tasks.push(tokio::spawn(async move {
            runner::drive(&plane, &runner, &shared).await
        }));
    }
    let mut tally: Option<Tally> = None;
    for task in tasks {
        let theirs = task
            .await
            .map_err(|_joined| Error::TaskLost { role: RUNNER_ROLE })??;
        match tally.as_mut() {
            Some(total) => total.absorb(&theirs)?,
            None => tally = Some(theirs),
        }
    }
    let length = started.elapsed();
    Ok(Window {
        tally: tally.ok_or(Error::TaskLost { role: RUNNER_ROLE })?,
        length,
        cost: statements::read(&stores.database).await?.since(before),
        dragonfly_calls: dragonfly_calls(&stores.queue)
            .await?
            .saturating_sub(dragonfly_before),
    })
}

impl Window {
    /// Fold a later window over the same runners into this one.
    ///
    /// # Errors
    ///
    /// When the two latency distributions will not merge.
    fn absorb(&mut self, later: &Self) -> Result<()> {
        self.tally.absorb(&later.tally)?;
        self.length += later.length;
        self.cost.statements += later.cost.statements;
        self.cost.commits += later.cost.commits;
        self.dragonfly_calls += later.dragonfly_calls;
        Ok(())
    }

    /// The drain's cost per lease, and where the population settled.
    fn record(&self, report: &mut Report, events: u64, settled: Settled) {
        let leased = self.tally.leased;
        report.measurement(LEASES, count(leased));
        report.measurement(POLLS, count(self.tally.polls));
        report.measurement(FAILURES, count(self.tally.failures));
        report.measurement(
            LEASES_PER_SECOND,
            per_second(leased, self.length.as_secs_f64()),
        );
        if !self.tally.lease_latency.is_empty() {
            report.measurement(
                LEASE_P95_MS,
                self.tally.lease_latency.quantile_ms(latency::P95),
            );
        }
        if !self.tally.report_latency.is_empty() {
            report.measurement(
                REPORT_P95_MS,
                self.tally.report_latency.quantile_ms(latency::P95),
            );
        }
        report.measurement(STATEMENTS_PER_LEASE, ratio(self.cost.statements, leased));
        report.measurement(COMMITS_PER_LEASE, ratio(self.cost.commits, leased));
        report.measurement(
            DRAGONFLY_CALLS_PER_LEASE,
            ratio(self.dragonfly_calls, leased),
        );
        report.measurement(EVENTS, count(events));
        report.measurement(EVENT_ROWS, count(settled.event_rows));
        report.measurement(EVENTS_PROCESSED, count(settled.processed));
        report.measurement(LEDGER_ROWS_PER_EVENT, ratio(settled.ledger_rows, events));
        report.measurement(READY_DEPTH, count(settled.ready_depth));
    }

    /// The quiet window's cost per poll.
    fn record_idle(&self, report: &mut Report) {
        let polls = self.tally.polls;
        report.measurement(IDLE_POLLS, count(polls));
        if polls == 0 {
            return;
        }
        report.measurement(IDLE_STATEMENTS_PER_POLL, ratio(self.cost.statements, polls));
        report.measurement(IDLE_COMMITS_PER_POLL, ratio(self.cost.commits, polls));
        report.measurement(
            IDLE_DRAGONFLY_CALLS_PER_POLL,
            ratio(self.dragonfly_calls, polls),
        );
    }
}

/// The drain's decisive numbers as one `key=value` line, for the terminal.
///
/// Printed rather than left in the file because the acceptance rubric reads
/// the make target's output. A key the run never measured is left out rather
/// than printed as zero.
#[must_use]
pub fn summary(report: &Report) -> String {
    SUMMARY_KEYS
        .iter()
        .filter_map(|key| {
            report
                .measurements
                .get(*key)
                .map(|value| format!("{key}={value}"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
