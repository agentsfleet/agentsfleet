//! Measurement and reporting for the contended and idle lease windows.

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_fleet::lease::Leases;

use super::drive::Shared;
use super::{
    ERROR_RATE, EXHAUSTED, FAILURES, IDLE_INDEX_DEPTH, IDLE_POLLS, IDLE_REDIS_CALLS_PER_POLL,
    IDLE_ROUNDTRIPS_PER_POLL, LEASES, POLLS_PER_SECOND, ROUNDTRIPS_PER_LEASE, WASTED_CLAIM_RATE,
    drive,
};
use crate::abort::Abort;
use crate::datastores::{Datastores, redis_calls};
use crate::error::{Error, Result};
use crate::instrument::{LeaseInstrument, PollCounters};
use crate::lane::outcomes::Outcomes;
use crate::report::{DatastoreCost, DatastoreCosts, Report, count, per_second, ratio};

/// One window: every runner polling at once, with the cost either side of it.
pub(super) struct Window {
    outcomes: Outcomes,
    length: Duration,
    exhausted: bool,
    counters: PollCounters,
    redis_calls: u64,
}

/// Drive every runner concurrently, measuring what it cost.
pub(super) async fn measure(
    instrument: &LeaseInstrument,
    leases: &Leases,
    stores: &Datastores,
    runners: &[Uuid7],
    window: Duration,
    stop_after: Option<u64>,
    abort: &Arc<Abort>,
) -> Result<Window> {
    let before = instrument.read()?;
    let redis_before = redis_calls(&stores.queue).await?;
    let started = Instant::now();
    let shared = Arc::new(Shared {
        deadline: started + window,
        leased: AtomicU64::new(0),
        stop_after,
        abort: Arc::clone(abort),
    });

    let mut tasks = Vec::with_capacity(runners.len());
    for runner in runners {
        let leases = leases.clone();
        let runner = runner.clone();
        let shared = Arc::clone(&shared);
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
    let ended = Instant::now();
    let exhausted = stop_after.is_some_and(|ceiling| shared.leased_so_far() >= ceiling);

    Ok(Window {
        outcomes,
        length: drive::window_length(started, ended, last_lease, exhausted),
        exhausted,
        counters: instrument.read()?.since(before),
        redis_calls: redis_calls(&stores.queue)
            .await?
            .saturating_sub(redis_before),
    })
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
        report.datastores = DatastoreCosts {
            redis: DatastoreCost {
                operations: self.redis_calls,
                time_ms: None,
            },
            postgres: DatastoreCost {
                operations: self.counters.roundtrips,
                time_ms: None,
            },
        };
    }

    /// Write the idle window's numbers, which are per-poll rather than a rate.
    pub(super) fn record_idle(&self, report: &mut Report, index_depth: u64) {
        let polls = self.outcomes.attempts();
        report.measurement(IDLE_INDEX_DEPTH, count(index_depth));
        report.measurement(IDLE_POLLS, count(polls));
        if polls == 0 {
            return;
        }
        report.measurement(
            IDLE_ROUNDTRIPS_PER_POLL,
            self.counters.roundtrips_per_poll(),
        );
        report.measurement(IDLE_REDIS_CALLS_PER_POLL, ratio(self.redis_calls, polls));
    }
}
