//! Measure complete production lease and report cycles, followed by idle polling.

pub(in crate::lane::lease) mod platform;
mod runner;
mod settled;
pub(in crate::lane::lease) mod stage;
mod summary;
mod window;

pub use self::summary::summary;

use core::time::Duration;
use std::sync::Arc;

use afd_fleet::lease::Plane;

use self::window::{Window, window};
use super::Parameters;
use crate::abort::Abort;
use crate::datastores::Datastores;
use crate::error::{ErrorKind, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::profile::Profile;
use crate::report::Report;

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

/// Measurement key: unsettled admissions remaining after the drain.
const UNSETTLED_ROWS: &str = "drain_unsettled_rows";

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
    UNSETTLED_ROWS,
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
    let plane = staged.plane(stores);
    let abort = Arc::new(Abort::new(profile.caps().abort_error_rate));
    let events = u64::try_from(staged.fleets.len()).unwrap_or(u64::MAX);
    let drained = drain(&plane, parameters, stores, staged, events, &abort).await?;
    let settled = settled::read(stores, &staged.fleets).await?;
    if settled.processed != events || drained.tally.failures != 0 {
        return Err(ErrorKind::DrainIncomplete {
            expected: events,
            processed: settled.processed,
            failures: drained.tally.failures,
        }
        .into());
    }
    drained.record(report, events, settled);
    // After an abort every runner exits on entry, and a per-poll ratio over a
    // window that never polled would describe nothing.
    if !abort.fired() {
        let idle = window(&plane, stores, &staged.runners, IDLE_WINDOW, None, &abort).await?;
        idle.record_idle(report);
    }
    Ok(())
}

/// Include the serving path's admission settlement in its drain cost.
async fn drain(
    plane: &Plane,
    parameters: Parameters,
    stores: &Datastores,
    staged: &stage::Staged,
    events: u64,
    abort: &Arc<Abort>,
) -> Result<Window> {
    window(
        plane,
        stores,
        &staged.runners,
        parameters.window,
        Some(events),
        abort,
    )
    .await
}

#[cfg(test)]
mod tests;
