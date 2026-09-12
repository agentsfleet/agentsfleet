//! What the drain measured, written into the report.
//!
//! Split from the lane at the file cap: the lane queues, drives and stops the
//! worker; this module turns what the poster stamped into numbers.

use core::time::Duration;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use super::poster::{Attempt, Behaviour, Scripted, Seen};
use crate::error::Result;
use crate::report::{
    AbortRecord, DatastoreCost, DatastoreCosts, Latency, Report, count, per_second,
};

/// Measurement key: jobs of this run the worker reached a terminal verdict on.
const DELIVERED: &str = "delivered";

/// Measurement key: jobs of this run still unsettled when the window ended.
const UNSETTLED: &str = "unsettled";

/// Measurement key: entries the worker met that were not this run's.
const FOREIGN: &str = "foreign_entries";

/// Measurement key: p95 delivery latency of jobs to destinations that were
/// NOT scripted slow — the head-of-line cost, isolated.
const OTHERS_P95_MS: &str = "others_p95_ms";

/// Measurement key: p95 delivery latency of the slow destinations themselves.
const SLOW_P95_MS: &str = "slow_p95_ms";

/// Measurement key: fraction of the window the worker sat in its retry ladder.
const RETRY_OCCUPANCY: &str = "retry_occupancy";

/// Measurement key: how many destinations were scripted slow.
const SLOW_DESTINATIONS: &str = "slow_destinations";

/// What the drain cost, either side of the worker's run.
pub(super) struct Drained {
    /// When the worker started: the instant every latency is measured from.
    pub(super) started: Instant,
    /// When the last of this run's jobs settled, or the deadline.
    pub(super) ended: Instant,
    pub(super) settled: u64,
    pub(super) redis_calls: u64,
    pub(super) transactions: u64,
}

/// When the window closed: the last answer this run's jobs received, which is
/// the last attempt stamped plus the delay the script gave it. Only when no
/// attempt was ever made does the deadline stand in.
pub(super) fn window_end(seen: &Seen, deadline: Instant) -> Instant {
    seen.attempts()
        .values()
        .filter_map(|attempts| attempts.last().map(Attempt::settled_at))
        .max()
        .unwrap_or(deadline)
}

/// Write the drain's numbers into the report.
pub(super) fn record(
    report: &mut Report,
    drained: &Drained,
    poster: &Scripted,
    queued_at: &HashMap<String, Instant>,
    behaviours: &BTreeMap<String, Behaviour>,
    jobs: u64,
) -> Result<()> {
    let seen = poster.seen();
    let mut all = Latency::new()?;
    let mut others = Latency::new()?;
    let mut slow = Latency::new()?;
    let mut ladder = Duration::ZERO;
    for (id, attempts) in seen.attempts() {
        let Some(first) = attempts.first() else {
            continue;
        };
        if let Some(queued) = queued_at.get(id) {
            // From the later of enqueue and worker start: time spent queued
            // before the worker existed is the harness's, not the worker's.
            let from = (*queued).max(drained.started);
            let latency = first.at.saturating_duration_since(from);
            all.record(latency)?;
            match first.behaviour {
                Behaviour::Slow => slow.record(latency)?,
                Behaviour::Fast | Behaviour::Retryable => others.record(latency)?,
            }
        }
        for (earlier, later) in attempts.iter().zip(attempts.iter().skip(1)) {
            ladder += later.at.saturating_duration_since(earlier.at);
        }
    }
    let elapsed = drained.ended.saturating_duration_since(drained.started);
    report.latency(elapsed, &all);
    report.count(DELIVERED, drained.settled);
    report.count(UNSETTLED, jobs.saturating_sub(drained.settled));
    report.count(FOREIGN, seen.foreign());
    if !others.is_empty() {
        report.calculated(
            OTHERS_P95_MS,
            others.quantile_ms(crate::report::latency::P95),
            crate::report::Calculation::quantile(&others, 950_000),
        );
    }
    if !slow.is_empty() {
        report.calculated(
            SLOW_P95_MS,
            slow.quantile_ms(crate::report::latency::P95),
            crate::report::Calculation::quantile(&slow, 950_000),
        );
    }
    report.calculated(
        RETRY_OCCUPANCY,
        crate::report::Calculation::duration_ratio_value(ladder, elapsed),
        crate::report::Calculation::Ratio {
            numerator: u64::try_from(ladder.as_nanos()).unwrap_or(u64::MAX),
            denominator: u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX),
        },
    );
    report.count(
        SLOW_DESTINATIONS,
        u64::try_from(
            behaviours
                .values()
                .filter(|b| **b == Behaviour::Slow)
                .count(),
        )
        .unwrap_or(u64::MAX),
    );
    // Rate is delivered-per-second over the window that delivered them; a
    // drain that ran out of window is recorded as an abort with the fraction
    // it left unsettled, never as a slow success.
    if drained.settled < jobs {
        report.abort = Some(AbortRecord {
            observed_error_rate: per_second(jobs.saturating_sub(drained.settled), count(jobs)),
            threshold: 0.0,
        });
    }
    report.datastores = DatastoreCosts {
        redis: DatastoreCost {
            operations: drained.redis_calls,
            time_ms: None,
        },
        postgres: DatastoreCost {
            operations: drained.transactions,
            time_ms: None,
        },
    };
    Ok(())
}
