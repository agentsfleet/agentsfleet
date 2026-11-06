//! Concurrent drain windows and their measured datastore cost.

use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_fleet::lease::Plane;

use super::runner::{self, Shared, Tally};
use super::settled::Settled;
use super::turn::Turn;
use super::{
    COMMITS_PER_LEASE, DRAGONFLY_CALLS_PER_LEASE, EVENT_ROWS, EVENTS, EVENTS_PROCESSED, FAILURES,
    IDLE_COMMITS_PER_POLL, IDLE_DRAGONFLY_CALLS_PER_POLL, IDLE_POLLS, IDLE_STATEMENTS_PER_POLL,
    LEASE_P95_MS, LEASES, LEASES_PER_SECOND, LEDGER_ROWS_PER_EVENT, POLLS, REPORT_P95_MS,
    RUNNER_ROLE, STATEMENTS_PER_LEASE, UNSETTLED_ROWS,
};
use crate::abort::Abort;
use crate::datastores::{Datastores, dragonfly_calls};
use crate::error::{ErrorKind, Result};
use crate::lane::joined;
use crate::report::{Report, count, latency, per_second, ratio};
use crate::statements::paths::{self, Path};
use crate::statements::{self, QueryCost, StatementCost};

/// One window of every runner at once, with its cost either side.
pub(super) struct Window {
    pub(super) tally: Tally,
    length: Duration,
    cost: StatementCost,
    /// Each statement's calls in the window, for the split by path.
    queries: Vec<QueryCost>,
    dragonfly_calls: u64,
}

/// Drive every runner until `target` events are reported or `length` passes.
pub(super) async fn window(
    plane: &Plane,
    stores: &Datastores,
    runners: &[Uuid7],
    length: Duration,
    target: Option<u64>,
    turn: Turn,
    abort: &Arc<Abort>,
) -> Result<Window> {
    let queries_before = statements::read_queries(&stores.database).await?;
    let before = statements::read(&stores.database).await?;
    let dragonfly_before = dragonfly_calls(&stores.queue).await?;
    let started = Instant::now();
    let shared = Arc::new(Shared {
        deadline: started + length,
        reported: 0.into(),
        target,
        turn,
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
        let theirs = joined(task.await, RUNNER_ROLE)??;
        match tally.as_mut() {
            Some(total) => total.absorb(&theirs)?,
            None => tally = Some(theirs),
        }
    }
    let length = started.elapsed();
    Ok(Window {
        tally: tally.ok_or(ErrorKind::TaskLost { role: RUNNER_ROLE })?,
        length,
        cost: statements::read(&stores.database).await?.since(before),
        queries: statements::read_queries(&stores.database)
            .await?
            .since(&queries_before),
        dragonfly_calls: dragonfly_calls(&stores.queue)
            .await?
            .saturating_sub(dragonfly_before),
    })
}

impl Window {
    /// The drain's cost per lease, and where the population settled.
    pub(super) fn record(&self, report: &mut Report, events: u64, settled: Settled) {
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
        if let Some(per) = paths::per_lease(&paths::by_path(&self.queries), leased) {
            for (path, value) in per {
                report.measurement(&path_key(path), value);
            }
        }
        report.measurement(COMMITS_PER_LEASE, ratio(self.cost.commits, leased));
        report.measurement(
            DRAGONFLY_CALLS_PER_LEASE,
            ratio(self.dragonfly_calls, leased),
        );
        report.measurement(EVENTS, count(events));
        report.measurement(EVENT_ROWS, count(settled.event_rows));
        report.measurement(EVENTS_PROCESSED, count(settled.processed));
        report.measurement(LEDGER_ROWS_PER_EVENT, ratio(settled.ledger_rows, events));
        report.measurement(UNSETTLED_ROWS, count(settled.unsettled_rows));
    }

    /// The quiet window's cost per poll.
    pub(super) fn record_idle(&self, report: &mut Report) {
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

/// The result key one path's statements per lease are written under.
fn path_key(path: Path) -> String {
    format!("{STATEMENTS_PER_LEASE}_{}", path.as_str())
}

impl Window {
    /// Name the statements that ran most per lease, so a reader of the run
    /// sees what each path's number is made of.
    pub(super) fn show_costliest(&self) {
        let leased = self.tally.leased.max(1);
        for cost in paths::costliest(&self.queries, COSTLIEST_SHOWN) {
            #[expect(
                clippy::cast_precision_loss,
                reason = "statement counts in one window stay far below 2^52"
            )]
            let per = cost.calls as f64 / leased as f64;
            let text: String = cost.query.split_whitespace().collect::<Vec<_>>().join(" ");
            // the bench has no subscriber; its stdout is the result summary, so the breakdown goes to stderr.
            eprintln!(
                "{per:>8.2}/lease  {:<8}  {}",
                paths::classify(&cost.query).as_str(),
                text.chars().take(QUERY_SHOWN).collect::<String>()
            );
        }
    }
}

/// How many statements a run names.
const COSTLIEST_SHOWN: usize = 5;

/// How much of a statement's text a run prints.
const QUERY_SHOWN: usize = 110;
