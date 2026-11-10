//! Concurrent drain windows and their measured datastore cost.

use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use afd_gateway::test_util::Session;

use super::burst::Pickups;
use super::runner::{self, Shared, Tally};
use super::settled::Settled;
use super::turn::Turn;
use super::{
    CLAIMS, COMMITS_PER_LEASE, DRAGONFLY_CALLS_PER_LEASE, EVENT_ROWS, EVENTS, EVENTS_PROCESSED,
    FAILURES, IDLE_COMMITS, IDLE_DRAGONFLY_CALLS, IDLE_STATEMENTS, LEASE_P95_MS, LEASES,
    LEASES_PER_SECOND, LEDGER_ROWS_PER_EVENT, REPORT_P95_MS, RUNNER_ROLE, STATEMENTS_PER_LEASE,
    UNSETTLED_ROWS,
};
use crate::abort::Abort;
use crate::datastores::{Datastores, dragonfly_calls};
use crate::error::Result;
use crate::lane::joined;
use crate::report::{Report, count, latency, per_second, ratio};
use crate::statements::paths::{self, Path};
use crate::statements::{self, QueryCost, StatementCost};

/// One window of every runner at once, with its cost either side.
pub(super) struct Window {
    pub(super) tally: Tally,
    pub(super) sessions: Vec<Session>,
    length: Duration,
    cost: StatementCost,
    /// Each statement's calls in the window, for the split by path.
    queries: Vec<QueryCost>,
    dragonfly_calls: u64,
}

/// What one window varies: how long it runs, how many events end it, how each
/// runner's turn behaves, and the pickups a burst's runners time.
pub(super) struct Shape {
    pub(super) length: Duration,
    pub(super) idle: bool,
    pub(super) target: Option<u64>,
    pub(super) turn: Turn,
    pub(super) pickups: Option<Arc<Pickups>>,
}

/// Drive every runner until `shape.target` events are reported or
/// `shape.length` passes.
pub(super) async fn window(
    sessions: Vec<Session>,
    stores: &Datastores,
    shape: Shape,
    abort: &Arc<Abort>,
) -> Result<Window> {
    let length = shape.length;
    let idle = shape.idle;
    let wake = afd_bus::wake::Wake::default();
    let wake_stop = tokio_util::sync::CancellationToken::new();
    let mut listeners = if idle {
        tokio::task::JoinSet::new()
    } else {
        wake.listen(&stores.bus, &wake_stop).await?
    };
    let queries_before = statements::read_queries(&stores.database).await?;
    let before = statements::read(&stores.database).await?;
    let dragonfly_before = dragonfly_calls(&stores.queue).await?;
    let started = Instant::now();
    let shared = Arc::new(Shared {
        deadline: started + length,
        reported: 0.into(),
        target: shape.target,
        turn: shape.turn,
        pickups: shape.pickups,
        abort: Arc::clone(abort),
        done: tokio_util::sync::CancellationToken::new(),
    });
    let measured = run_sessions(sessions, &shared, &wake, idle).await;
    wake_stop.cancel();
    listeners.shutdown().await;
    let (tally, sessions) = measured?;
    let length = started.elapsed();
    Ok(Window {
        tally,
        sessions,
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

/// Each host keeps its own in-process session across the measured windows.
async fn run_sessions(
    sessions: Vec<Session>,
    shared: &Arc<Shared>,
    wake: &afd_bus::wake::Wake,
    idle: bool,
) -> Result<(Tally, Vec<Session>)> {
    let mut tasks = tokio::task::JoinSet::new();
    for session in sessions {
        let shared = Arc::clone(shared);
        let generations = wake.generations();
        tasks.spawn(async move {
            if idle {
                let abort = shared.abort.token();
                tokio::select! {
                    () = abort.cancelled() => {},
                    () = tokio::time::sleep_until(shared.deadline.into()) => {},
                }
                Ok((Tally::new()?, session))
            } else {
                runner::drive(session, &shared, generations).await
            }
        });
    }
    let mut tally = Tally::new()?;
    let mut sessions = Vec::new();
    let result = async {
        while let Some(task) = tasks.join_next().await {
            let (theirs, session) = joined(task, RUNNER_ROLE)??;
            tally.absorb(&theirs)?;
            sessions.push(session);
        }
        Ok::<_, crate::error::Error>(())
    }
    .await;
    if result.is_err() {
        tasks.shutdown().await;
    }
    result?;
    Ok((tally, sessions))
}

impl Window {
    /// The drain's cost per lease, and where the population settled.
    pub(super) fn record(&self, report: &mut Report, events: u64, settled: Settled) {
        let leased = self.tally.leased;
        report.measurement(LEASES, count(leased));
        report.measurement(CLAIMS, count(self.tally.claims));
        report.measurement(FAILURES, 0.0);
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

    /// Idle sessions report measured totals even when no offer pass ran.
    pub(super) fn record_idle(&self, report: &mut Report) {
        report.measurement(IDLE_STATEMENTS, count(self.cost.statements));
        report.measurement(IDLE_COMMITS, count(self.cost.commits));
        report.measurement(IDLE_DRAGONFLY_CALLS, count(self.dragonfly_calls));
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
            let per = crate::report::ratio(cost.calls, leased);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;
    use crate::report::{Lane, Provenance};

    #[test]
    fn an_idle_window_reports_totals_not_ratios() -> Result<()> {
        let window = Window {
            tally: Tally::new()?,
            sessions: Vec::new(),
            length: Duration::from_secs(2),
            cost: StatementCost {
                statements: 0,
                commits: 0,
            },
            queries: Vec::new(),
            dragonfly_calls: 0,
        };
        let mut report = Report::new(Lane::Lease, Profile::Rig, Provenance::for_test());
        window.record_idle(&mut report);
        assert_eq!(report.measurements.get(IDLE_STATEMENTS), Some(&0.0));
        assert_eq!(report.measurements.get(IDLE_DRAGONFLY_CALLS), Some(&0.0));
        assert_eq!(report.measurements.len(), 3);
        Ok(())
    }
}
