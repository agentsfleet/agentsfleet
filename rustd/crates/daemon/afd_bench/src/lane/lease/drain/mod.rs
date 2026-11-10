//! Measure complete production lease and report cycles, followed by connected idle sessions.

pub mod burst;
pub(in crate::lane::lease) mod native;
pub(in crate::lane::lease) mod platform;
mod runner;
pub(in crate::lane::lease) mod session;
mod settled;
pub(in crate::lane::lease) mod stage;
mod summary;
pub(in crate::lane::lease) mod turn;
mod window;

pub use self::summary::summary;

use core::time::Duration;
use std::sync::Arc;

use afd_gateway::test_util::Session;

use self::turn::Turn;
use self::window::{Shape, Window, window};
use super::Parameters;
use crate::abort::Abort;
use crate::datastores::Datastores;
use crate::error::{ErrorKind, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::profile::Profile;
use crate::report::Report;

/// Measurement key: leases the drain issued, one per event it settled.
const LEASES: &str = "drain_leases";

/// Measurement key: production offer entries driven by notifications and capacity.
const CLAIMS: &str = "drain_offer_claims";

/// Measurement key: zero when every native operation completed.
const FAILURES: &str = "drain_failures";

/// Measurement key: leases issued per second of the drain.
const LEASES_PER_SECOND: &str = "drain_leases_per_second";

/// Measurement key: the tail of a pushed offer.
const LEASE_P95_MS: &str = "drain_lease_p95_ms";

/// Measurement key: the tail of the report after it.
const REPORT_P95_MS: &str = "drain_report_p95_ms";

/// Measurement key: statements per lease across offer, native exchanges and completion.
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

/// Measurement key: unsettled admissions remaining after the drain.
const UNSETTLED_ROWS: &str = "drain_unsettled_rows";

/// Measurement key: statements served while sessions have no work.
pub const IDLE_STATEMENTS: &str = "drain_idle_statements";
/// Measurement key: commits served while sessions have no work.
const IDLE_COMMITS: &str = "drain_idle_commits";
/// Measurement key: broker commands served while sessions have no work.
const IDLE_DRAGONFLY_CALLS: &str = "drain_idle_dragonfly_calls";

/// The task role a lost drain runner is reported under.
const RUNNER_ROLE: &str = "drain runner";

/// How long sessions remain idle after the drain.
///
/// Uses the same duration as the contended window's idle interval.
const IDLE_WINDOW: Duration = Duration::from_secs(2);

/// The keys the one-line summary prints, in order.
const SUMMARY_KEYS: [&str; 5] = [
    IDLE_STATEMENTS,
    IDLE_COMMITS,
    STATEMENTS_PER_LEASE,
    COMMITS_PER_LEASE,
    UNSETTLED_ROWS,
];

/// Drain a fresh population through one session per host, then measure connected idle sessions.
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
    let released = platform::release(stores, stage::INDEX_BASE).await;
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
    let sessions = staged.sessions(stores).await?;
    let abort = Arc::new(Abort::new(profile.caps().abort_error_rate));
    let events = u64::try_from(staged.fleets.len())?;
    let drained = drain(sessions, parameters, stores, events, &abort).await?;
    let settled = settled::read(stores, &staged.fleets).await?;
    if settled.processed != events {
        return Err(ErrorKind::DrainIncomplete {
            expected: events,
            processed: settled.processed,
            failures: 0,
        }
        .into());
    }
    drained.record(report, events, settled);
    drained.show_costliest();
    // An aborted measurement cannot establish the cost of a complete idle interval.
    if !abort.fired() {
        let quiet = Turn::back_to_back(Duration::ZERO);
        let idle = window(
            drained.sessions,
            stores,
            Shape {
                length: IDLE_WINDOW,
                idle: true,
                target: None,
                turn: quiet,
                pickups: None,
            },
            &abort,
        )
        .await?;
        idle.record_idle(report);
        for session in idle.sessions {
            session.close().await;
        }
    }
    Ok(())
}

/// Include the serving path's admission settlement in its drain cost.
async fn drain(
    sessions: Vec<Session>,
    parameters: Parameters,
    stores: &Datastores,
    events: u64,
    abort: &Arc<Abort>,
) -> Result<Window> {
    window(
        sessions,
        stores,
        Shape {
            length: parameters.window,
            idle: false,
            target: Some(events),
            turn: Turn::back_to_back(parameters.turn),
            pickups: None,
        },
        abort,
    )
    .await
}

#[cfg(test)]
mod tests;
