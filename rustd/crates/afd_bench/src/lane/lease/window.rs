//! One measured window: every runner polling the assignment pass at once, and
//! the cost either side of it.
//!
//! Split from the lane's orchestration at the file cap, along the seam the
//! lane already had: `lease.rs` decides WHAT runs in which order, and this
//! decides how one window is driven, counted and written into the report.

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_fleet::lease::Leases;

use super::drive::{self, Shared};
use crate::abort::Abort;
use crate::datastores::{Datastores, dragonfly_calls};
use crate::error::{Error, Result};
use crate::instrument::{LeaseInstrument, PollCounters};
use crate::lane::outcomes::Outcomes;
use crate::report::{DatastoreCost, DatastoreCosts, Report, count, per_second, ratio};

/// Measurement key: polls issued per second, lease or miss.
const POLLS_PER_SECOND: &str = "polls_per_second";

/// Measurement key: how many leases the window issued in total.
const LEASES: &str = "leases";

/// Measurement key: polls the path refused.
const FAILURES: &str = "failures";

/// Measurement key: the fraction of everything tried that the path refused.
const ERROR_RATE: &str = "error_rate";

/// Measurement key: Postgres round trips the pass made per issued lease.
const ROUNDTRIPS_PER_LEASE: &str = "roundtrips_per_lease";

/// Measurement key: the fraction of polls that produced no work.
const WASTED_CLAIM_RATE: &str = "wasted_claim_rate";

/// Measurement key: whether the contended window ended at exhaustion (1) or
/// at its deadline (0), so a reader knows which the rate is over.
const EXHAUSTED: &str = "exhausted";

/// One window: every runner polling at once, with the cost either side of it.
pub(super) struct Window {
    outcomes: Outcomes,
    length: Duration,
    exhausted: bool,
    counters: PollCounters,
    dragonfly_calls: u64,
}

/// What both of a run's windows drive and read: the same runners, the same
/// lease path, the same counters and the same abort monitor.
pub(super) struct Pollers<'run> {
    /// The daemon's own round-trip counters.
    pub(super) instrument: &'run LeaseInstrument,
    /// The lease path under measurement.
    pub(super) leases: &'run Leases,
    /// The datastores, for Dragonfly's command count.
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
        let before = self.instrument.read()?;
        let dragonfly_before = dragonfly_calls(&self.stores.queue).await?;
        let started = Instant::now();
        let shared = Arc::new(Shared {
            deadline: started + window,
            leased: AtomicU64::new(0),
            stop_after,
            abort: Arc::clone(self.abort),
        });

        let (outcomes, last_lease) = drive_all(self.leases, self.runners, &shared).await?;
        let ended = Instant::now();
        let exhausted = stop_after.is_some_and(|ceiling| shared.leased_so_far() >= ceiling);

        Ok(Window {
            outcomes,
            length: drive::window_length(started, ended, last_lease, exhausted),
            exhausted,
            counters: self.instrument.read()?.since(before),
            dragonfly_calls: dragonfly_calls(&self.stores.queue)
                .await?
                .saturating_sub(dragonfly_before),
        })
    }
}

/// Every runner polling at once, answering what they did together and when
/// the last of them issued a lease.
async fn drive_all(
    leases: &Leases,
    runners: &[Uuid7],
    shared: &Arc<Shared>,
) -> Result<(Outcomes, Option<Instant>)> {
    let mut tasks = Vec::with_capacity(runners.len());
    for runner in runners {
        let leases = leases.clone();
        let runner = runner.clone();
        let shared = Arc::clone(shared);
        // Per runner, because the thing under measurement is what happens when
        // R of them reach the same readiness index at the same instant.
        tasks.push(tokio::spawn(async move {
            drive::poll_until(&leases, &runner, &shared).await
        }));
    }

    let mut outcomes = Outcomes::new()?;
    let mut last_lease: Option<Instant> = None;
    for task in tasks {
        let (theirs, their_last) = task
            .await
            .map_err(|_joined| Error::TaskLost { role: "runner" })??;
        outcomes.absorb(&theirs)?;
        last_lease = last_lease.max(their_last);
    }
    Ok((outcomes, last_lease))
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
        report.measurement(LEASES, count(self.outcomes.successes));
        report.measurement(FAILURES, count(self.outcomes.failures));
        report.measurement(ERROR_RATE, self.outcomes.failure_fraction());
        report.measurement(EXHAUSTED, if self.exhausted { 1.0 } else { 0.0 });
        report.measurement(WASTED_CLAIM_RATE, self.outcomes.wasted_fraction());
        report.measurement(
            ROUNDTRIPS_PER_LEASE,
            ratio(self.counters.roundtrips, self.outcomes.successes),
        );
        // No `time_ms`: this lane times the poll end to end, which is already
        // the p95, and splitting that between the two datastores would need a
        // timer inside the pass rather than around it.
        report.datastores = DatastoreCosts {
            dragonfly: DatastoreCost {
                operations: self.dragonfly_calls,
                time_ms: None,
            },
            postgres: DatastoreCost {
                operations: self.counters.roundtrips,
                time_ms: None,
            },
        };
    }
}
