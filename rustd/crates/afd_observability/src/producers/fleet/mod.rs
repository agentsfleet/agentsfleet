//! What the runner plane, the sweepers and the account paths record.
//!
//! The handles are claimed here and the recording functions are split by the
//! path that calls them: [`runner`] is the per-runner table's, and what is left here is the account, lease-poll,
//! readiness and retention paths — each of which is one or two call sites.

pub mod admission;
pub mod census;
pub mod history;
pub mod hold;
pub mod runner;

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram};
use std::sync::Arc;

use crate::error::Result;
use crate::metrics::declared::fleet as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::label::fleet::{DeliveryStage, ReadyWake, RunStart, SignupFailure};
use crate::metrics::observed::Observed;
use crate::producers::installed;
use crate::runner::RunnerMetrics;
use crate::semconv;

/// The `reason` value a failure class this build does not model counts under.
///
/// A newer runner's class must still be COUNTED, or the failure total stops
/// agreeing with the sum of its reasons and an operator goes looking for the
/// difference rather than for the failure.
pub(super) const UNMODELLED_REASON: &str = "unknown";

/// How many fleets the last lease poll found holding work.
///
/// Published by the poll rather than read from Dragonfly in the callback: the
/// index is a network round trip, and a collection callback runs under the
/// SDK's pipeline lock where an await is not available and a stall would take
/// every family silent at once.
static READY_DEPTH: Observed = Observed::new();

/// Admitted rows the last replay pass found without a receipt.
static ADMISSION_BACKLOG: Observed = Observed::new();

/// How long the oldest of them had waited, in seconds.
static ADMISSION_OLDEST: Observed = Observed::new();

/// Publishes what a completed lease poll saw in the readiness index.
pub fn ready_depth_observed(fleets: u64) {
    READY_DEPTH.publish(fleets);
}

/// Publishes what a completed replay pass found the queue still owing.
///
/// Both readings together, because they describe one backlog: rows without
/// an age cannot say whether the sweeper is keeping up. An empty backlog
/// publishes zero rows and zero age, which is a measurement, not a gap.
pub fn admission_backlog_observed(rows: u64, oldest_age_seconds: u64) {
    ADMISSION_BACKLOG.publish(rows);
    ADMISSION_OLDEST.publish(oldest_age_seconds);
}

/// The instruments the runner plane and the sweepers record through.
#[derive(Debug)]
pub struct Handles {
    signup_bootstrapped: Counter<u64>,
    signup_replayed: Counter<u64>,
    signup_failed: Counter<u64>,
    lease_polls: Counter<u64>,
    lease_candidates: Counter<u64>,
    lease_roundtrips: Counter<u64>,
    lease_dragonfly_commands: Counter<u64>,
    lease_claims_empty: Counter<u64>,
    history_bytes: Histogram<f64>,
    history_cuts: Counter<u64>,
    history_read_failures: Counter<u64>,
    held_claims: Counter<u64>,
    runs_started: Counter<u64>,
    delivery_stage: Histogram<f64>,
    ready_write_failures: Counter<u64>,
    ready_wake_failures: Counter<u64>,
    retention_swept: Counter<u64>,
    retention_failures: Counter<u64>,
    admissions: Counter<u64>,
    admission_replays: Counter<u64>,
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
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        let runners = Arc::new(RunnerMetrics::new());
        let handles = Self {
            runners: Arc::clone(&runners),
            signup_bootstrapped: instruments.counter_u64(&declared::SIGNUP_BOOTSTRAPPED_TOTAL)?,
            signup_replayed: instruments.counter_u64(&declared::SIGNUP_REPLAYED_TOTAL)?,
            signup_failed: instruments.counter_u64(&declared::SIGNUP_FAILED_TOTAL)?,
            lease_polls: instruments.counter_u64(&declared::LEASE_POLLS_TOTAL)?,
            lease_candidates: instruments
                .counter_u64(&declared::LEASE_POLL_CANDIDATES_SCANNED_TOTAL)?,
            lease_roundtrips: instruments.counter_u64(&declared::LEASE_POLL_DB_ROUNDTRIPS_TOTAL)?,
            lease_dragonfly_commands: instruments
                .counter_u64(&declared::LEASE_POLL_DRAGONFLY_COMMANDS_TOTAL)?,
            lease_claims_empty: instruments.counter_u64(&declared::LEASE_CLAIMS_EMPTY_TOTAL)?,
            history_bytes: instruments.histogram_f64(&declared::LEASE_HISTORY_BYTES)?,
            history_cuts: instruments.counter_u64(&declared::LEASE_HISTORY_CUTS_TOTAL)?,
            history_read_failures: instruments
                .counter_u64(&declared::LEASE_HISTORY_READ_FAILURES_TOTAL)?,
            held_claims: instruments.counter_u64(&declared::LEASE_HELD_CLAIMS_TOTAL)?,
            runs_started: instruments.counter_u64(&declared::FLEET_RUNS_STARTED_TOTAL)?,
            delivery_stage: instruments.histogram_f64(&declared::FLEET_DELIVERY_STAGE_SECONDS)?,
            ready_write_failures: instruments
                .counter_u64(&declared::FLEET_READY_WRITE_FAILURES_TOTAL)?,
            ready_wake_failures: instruments
                .counter_u64(&declared::FLEET_READY_WAKE_FAILURES_TOTAL)?,
            retention_swept: instruments.counter_u64(&declared::RUNNER_RETENTION_SWEPT_TOTAL)?,
            retention_failures: instruments
                .counter_u64(&declared::RUNNER_RETENTION_SWEEP_FAILURES_TOTAL)?,
            admissions: instruments.counter_u64(&declared::ADMISSIONS_TOTAL)?,
            admission_replays: instruments.counter_u64(&declared::ADMISSION_REPLAYS_TOTAL)?,
            runner_failures: instruments.counter_u64(&declared::RUNNER_FAILURES_TOTAL)?,
            runner_failures_overflow: instruments
                .counter_u64(&declared::RUNNER_FAILURES_OVERFLOW_TOTAL)?,
            runner_executions: instruments.counter_u64(&declared::RUNNER_EXECUTIONS_TOTAL)?,
        };
        Self::observe_gauges(instruments, &runners)?;
        Ok(handles)
    }

    /// Registers every gauge this domain observes, after the counters above.
    ///
    /// The backlog gauges read a snapshot cell their publisher writes; the two
    /// runner gauges read the same bounded table the counters admit runners
    /// into, so each takes its own handle on `runners`.
    ///
    /// # Errors
    ///
    /// Whatever [`Instruments`] refuses — see [`super::install`].
    fn observe_gauges(instruments: &Instruments, runners: &Arc<RunnerMetrics>) -> Result<()> {
        instruments.gauge_u64(&declared::FLEETS, census::fleet_census_readings)?;
        instruments.gauge_u64(&declared::FLEET_READY_DEPTH, unlabelled(&READY_DEPTH))?;
        instruments.gauge_u64(&declared::ADMISSION_BACKLOG, unlabelled(&ADMISSION_BACKLOG))?;
        instruments.gauge_u64(
            &declared::ADMISSION_BACKLOG_OLDEST_AGE_SECONDS,
            unlabelled(&ADMISSION_OLDEST),
        )?;

        let seen = Arc::clone(runners);
        instruments.gauge_u64(&declared::RUNNER_LAST_SEEN_SECONDS, move || {
            seen.last_seen_readings()
        })?;

        let leased = Arc::clone(runners);
        instruments.gauge_u64(&declared::RUNNER_ACTIVE_LEASES, move || {
            leased.active_lease_readings()
        })?;
        Ok(())
    }
}

/// The collection callback for one snapshot cell: its reading, unlabelled, or
/// nothing when no publisher has vouched for one.
fn unlabelled(cell: &'static Observed) -> impl Fn() -> Vec<Reading> + Send + Sync + 'static {
    move || cell.load().into_iter().map(Reading::unlabelled).collect()
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
/// All four counters move together, on every exit path, because the ratio is
/// the number worth reading: candidates per poll says how much work a poll
/// examined, round-trips per poll says how much of that reached Postgres, and
/// commands per poll says what it asked of Dragonfly. A poll that recorded
/// only some of them would skew the rest.
pub fn lease_polled(candidates_scanned: u64, database_roundtrips: u64, dragonfly_commands: u64) {
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
        producers
            .fleet
            .lease_dragonfly_commands
            .add(dragonfly_commands, &[]);
    }
}

/// Records a won claim whose fleet had nothing deliverable.
///
/// The cost a drained fleet adds before its mark is cleared: one claim, one
/// stream read, one release. A rate that stays up while runs do not start is
/// marks that keep coming back — a writer re-marking fleets with no work.
pub fn lease_claimed_empty() {
    if let Some(producers) = installed() {
        producers.fleet.lease_claims_empty.add(1, &[]);
    }
}

/// Records one run started: a lease granted, fresh or by reclaim.
///
/// Called at the one grant point, on the `Some` it returns. A poll that found
/// nothing leasable records nothing, because nothing started.
pub fn run_started(kind: RunStart) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .runs_started
            .add(1, &[KeyValue::new(semconv::LABEL_KIND, kind.as_str())]);
    }
}

/// Records one completed delivery stage. A negative cross-clock difference is
/// omitted by the caller, so Grafana never sees a fabricated zero.
pub fn delivery_stage(stage: DeliveryStage, elapsed: core::time::Duration) {
    if let Some(producers) = installed() {
        producers.fleet.delivery_stage.record(
            elapsed.as_secs_f64(),
            &[KeyValue::new(semconv::LABEL_STAGE, stage.as_str())],
        );
    }
}

/// Records a readiness mark or clear the index would not accept.
pub fn ready_write_failed() {
    if let Some(producers) = installed() {
        producers.fleet.ready_write_failures.add(1, &[]);
    }
}

/// Records a readiness-doorbell ring or rung peek that failed. The work
/// behind it is never lost: the rotation still visits every partition.
pub fn ready_wake_failed(wake: ReadyWake) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .ready_wake_failures
            .add(1, &[KeyValue::new(semconv::LABEL_KIND, wake.as_str())]);
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
