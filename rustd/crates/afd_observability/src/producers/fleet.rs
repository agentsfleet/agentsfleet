//! What the runner plane, the sweepers and the account paths record.
//!
//! The handles are claimed here and the recording functions are split by the
//! path that calls them: [`repair`] is the verification pipeline's, [`runner`]
//! is the per-runner table's, and what is left here is the account, lease-poll,
//! readiness and retention paths — each of which is one or two call sites.

pub mod repair;
pub mod runner;

use std::sync::Arc;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram};

use crate::error::Result;
use crate::metrics::declared::fleet as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::label::fleet::SignupFailure;
use crate::producers::{GaugeSources, installed};
use crate::runner::RunnerMetrics;
use crate::semconv;

/// The `reason` value a failure class this build does not model counts under.
///
/// A newer runner's class must still be COUNTED, or the failure total stops
/// agreeing with the sum of its reasons and an operator goes looking for the
/// difference rather than for the failure.
pub(super) const UNMODELLED_REASON: &str = "unknown";

/// The instruments the runner plane and the sweepers record through.
#[derive(Debug)]
pub struct Handles {
    signup_bootstrapped: Counter<u64>,
    signup_replayed: Counter<u64>,
    signup_failed: Counter<u64>,
    lease_polls: Counter<u64>,
    lease_candidates: Counter<u64>,
    lease_roundtrips: Counter<u64>,
    ready_write_failures: Counter<u64>,
    retention_swept: Counter<u64>,
    retention_failures: Counter<u64>,
    repair_provider_results: Counter<u64>,
    repair_correlations: Counter<u64>,
    repair_intents: Counter<u64>,
    repair_retries: Counter<u64>,
    repair_events: Counter<u64>,
    repair_runs: Counter<u64>,
    repair_to_queue: Histogram<f64>,
    repair_to_completion: Histogram<f64>,
    runner_failures: Counter<u64>,
    runner_failures_overflow: Counter<u64>,
    runner_executions: Counter<u64>,
    /// The bounded per-runner table both the counters and the two runner
    /// gauges read.
    ///
    /// Owned here rather than supplied by boot, because it is telemetry state
    /// and nothing else reads it: a copy threaded in from outside would be a
    /// second table that could disagree with this one about which runners are
    /// admitted, and the admission decision is the whole cardinality bound.
    pub(super) runners: Arc<RunnerMetrics>,
}

impl Handles {
    /// Claims every instrument this domain records through.
    ///
    /// # Errors
    ///
    /// Whatever [`Instruments`] refuses — see [`super::install`].
    pub(super) fn claim(instruments: &Instruments, sources: &GaugeSources) -> Result<Self> {
        let runners = Arc::new(RunnerMetrics::new());
        let handles = Self {
            runners: Arc::clone(&runners),
            signup_bootstrapped: instruments.counter_u64(declared::SIGNUP_BOOTSTRAPPED_TOTAL)?,
            signup_replayed: instruments.counter_u64(declared::SIGNUP_REPLAYED_TOTAL)?,
            signup_failed: instruments.counter_u64(declared::SIGNUP_FAILED_TOTAL)?,
            lease_polls: instruments.counter_u64(declared::LEASE_POLLS_TOTAL)?,
            lease_candidates: instruments
                .counter_u64(declared::LEASE_POLL_CANDIDATES_SCANNED_TOTAL)?,
            lease_roundtrips: instruments
                .counter_u64(declared::LEASE_POLL_DB_ROUNDTRIPS_TOTAL)?,
            ready_write_failures: instruments
                .counter_u64(declared::FLEET_READY_WRITE_FAILURES_TOTAL)?,
            retention_swept: instruments.counter_u64(declared::RUNNER_RETENTION_SWEPT_TOTAL)?,
            retention_failures: instruments
                .counter_u64(declared::RUNNER_RETENTION_SWEEP_FAILURES_TOTAL)?,
            repair_provider_results: instruments
                .counter_u64(declared::REPAIR_PROVIDER_RESULTS_TOTAL)?,
            repair_correlations: instruments.counter_u64(declared::REPAIR_CORRELATIONS_TOTAL)?,
            repair_intents: instruments
                .counter_u64(declared::REPAIR_VERIFICATION_INTENTS_CREATED_TOTAL)?,
            repair_retries: instruments.counter_u64(declared::REPAIR_DISPATCH_RETRIED_TOTAL)?,
            repair_events: instruments.counter_u64(declared::REPAIR_SYNTHETIC_EVENTS_TOTAL)?,
            repair_runs: instruments.counter_u64(declared::REPAIR_VERIFIER_RUNS_TOTAL)?,
            repair_to_queue: instruments
                .histogram_f64(declared::REPAIR_PRODUCTION_TO_QUEUE_SECONDS)?,
            repair_to_completion: instruments
                .histogram_f64(declared::REPAIR_QUEUE_TO_COMPLETION_SECONDS)?,
            runner_failures: instruments.counter_u64(declared::RUNNER_FAILURES_TOTAL)?,
            runner_failures_overflow: instruments
                .counter_u64(declared::RUNNER_FAILURES_OVERFLOW_TOTAL)?,
            runner_executions: instruments.counter_u64(declared::RUNNER_EXECUTIONS_TOTAL)?,
        };

        let ready = Arc::clone(&sources.ready_fleets);
        instruments.gauge_u64(declared::FLEET_READY_DEPTH, move || {
            ready().into_iter().map(Reading::unlabelled).collect()
        })?;

        let due = Arc::clone(&sources.repair_due_batch);
        instruments.gauge_u64(declared::REPAIR_DISPATCH_DUE_BATCH, move || {
            due().into_iter().map(Reading::unlabelled).collect()
        })?;

        let oldest = Arc::clone(&sources.repair_oldest_age);
        instruments.gauge_u64(declared::REPAIR_DISPATCH_OLDEST_AGE_SECONDS, move || {
            oldest().into_iter().map(Reading::unlabelled).collect()
        })?;

        let seen = Arc::clone(&runners);
        instruments.gauge_u64(declared::RUNNER_LAST_SEEN_SECONDS, move || {
            seen.last_seen_readings()
        })?;

        let leased = Arc::clone(&runners);
        instruments.gauge_u64(declared::RUNNER_ACTIVE_LEASES, move || {
            leased.active_lease_readings()
        })?;

        Ok(handles)
    }
}

/// Records an account opened from a verified signup delivery.
pub fn signup_bootstrapped() {
    if let Some(producers) = installed() {
        producers.fleet.signup_bootstrapped.add(1, &[]);
    }
}

/// Records a delivery for an account that already existed.
pub fn signup_replayed() {
    if let Some(producers) = installed() {
        producers.fleet.signup_replayed.add(1, &[]);
    }
}

/// Records a signup this daemon did not turn into an account.
pub fn signup_failed(reason: SignupFailure) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .signup_failed
            .add(1, &[KeyValue::new(semconv::LABEL_REASON, reason.as_str())]);
    }
}

/// Records one finished lease poll and what it cost.
///
/// All three counters move together, on every exit path, because the ratio is
/// the number worth reading: candidates per poll says how much work a poll
/// examined, and round-trips per poll says how much of that reached Postgres.
/// A poll that recorded only some of them would skew both.
pub fn lease_polled(candidates_scanned: u64, database_roundtrips: u64) {
    if let Some(producers) = installed() {
        producers.fleet.lease_polls.add(1, &[]);
        producers
            .fleet
            .lease_candidates
            .add(candidates_scanned, &[]);
        producers
            .fleet
            .lease_roundtrips
            .add(database_roundtrips, &[]);
    }
}

/// Records a readiness mark or clear the index would not accept.
pub fn ready_write_failed() {
    if let Some(producers) = installed() {
        producers.fleet.ready_write_failures.add(1, &[]);
    }
}

/// Records what one retention pass deleted.
pub fn retention_swept(rows: u64) {
    if let Some(producers) = installed() {
        producers.fleet.retention_swept.add(rows, &[]);
    }
}

/// Records a retention pass that did not finish.
pub fn retention_failed() {
    if let Some(producers) = installed() {
        producers.fleet.retention_failures.add(1, &[]);
    }
}
