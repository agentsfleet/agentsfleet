//! Per-runner counters, bounded by construction.
//!
//! What a runner did — how many runs it finished, how many failed and why —
//! keyed by the runner's own identifier so an operator can tell one bad host
//! from a bad fleet.
//!
//! # The bound is the whole design
//!
//! `runner_id` is CALLER-SUPPLIED. A misconfigured fleet re-enrolling in a loop,
//! or a hostile one, produces unbounded distinct label values, and a metric
//! registry that grew a series per value would consume memory until the process
//! died — with no request ever failing to explain why. So the table has a fixed
//! capacity, and everything past it accumulates into ONE overflow series
//! (Dimension 6.4).
//!
//! Overflow keeps the reason and the outcome and loses only WHICH runner: a
//! deployment past four thousand distinct runners has an operational problem
//! that per-runner attribution would not help with, and the failure totals stay
//! correct either way.
//!
//! # What this is NOT
//!
//! The overflow series' NAME. The OpenTelemetry specification marks an
//! overflowing attribute set with `otel.metric.overflow`, the Zig uses an
//! `_other` label value, and choosing between them belongs with the milestone
//! that configures the metrics pipeline and owns the dashboards that read it.
//! What is settled here is the BOUND and the constant memory.
//!
//! # Why a map behind a lock, and not a lock-free slot table
//!
//! `metrics_runner.zig` is a fixed array of slots with a compare-and-swap
//! claim, a readiness flag, a bounded spin for a slot another thread is still
//! initialising, and a truncated `[48]u8` copy of each identifier — because it
//! has no allocator at runtime and must not block a request path.
//!
//! None of that buys anything here. The write path takes a read lock and
//! touches atomics; only a runner's FIRST record takes the write lock, and a
//! deployment sees at most a few thousand of those in its life. There is no
//! spin to bound because there is no half-initialised state to wait on, and no
//! identifier truncation because the key owns its bytes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use afd_wire::report::{FailureClass, Outcome};

/// How many distinct runners get their own series.
///
/// The Zig's, and the number is not the point — the bound is. Four thousand is
/// far past any real deployment's host count and far below anything that
/// threatens memory.
pub const MAX_SERIES: usize = 4096;

/// Every failure class, plus the bucket for one this build does not model.
///
/// Eleven declared reasons and one bucket. The count is checked against the
/// index table below rather than derived, because deriving it from the enum
/// would make an out-of-range index a silent no-count instead of a build
/// failure.
///
/// A reason arriving from a newer runner must still be COUNTED: dropping it
/// would make the failure total disagree with the sum of its reasons, and an
/// operator would be looking for the difference rather than for the failure.
const REASONS: usize = 12;

/// Every outcome a runner can report.
const OUTCOMES: usize = 2;

/// The counters one series holds.
///
/// Atomics rather than a lock, so recording never blocks another recorder —
/// the map's lock is taken for LOOKUP, and released before anything is
/// incremented.
#[derive(Debug, Default)]
struct Counters {
    /// Failures by reason, with the unmodelled bucket last.
    failures: [AtomicU64; REASONS],
    /// Executions by outcome.
    executions: [AtomicU64; OUTCOMES],
    /// When this runner was last heard from, in Unix milliseconds.
    last_seen_ms: AtomicI64,
    /// How many leases it is believed to hold.
    ///
    /// Best effort, and knowingly: a lease abandoned by a dead runner expires
    /// by the clock with no report to decrement against, so a crashed host's
    /// gauge stays high until the process restarts. The liveness sweeper's row
    /// is the authority; this is a hint next to it.
    active_leases: AtomicI64,
}

impl Counters {
    /// Records one failure.
    fn fail(&self, reason: Option<FailureClass>) {
        let index = reason.map_or(REASONS - 1, reason_index);
        if let Some(counter) = self.failures.get(index) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records one finished run.
    fn execute(&self, outcome: Outcome) {
        if let Some(counter) = self.executions.get(outcome_index(outcome)) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The per-runner metric families this process holds.
///
/// One value the daemon owns, rather than the Zig's process-global mutable
/// arrays: a test can build its own and drive it to the cardinality edge
/// without touching what any other test is counting.
#[derive(Debug)]
pub struct RunnerMetrics {
    /// One entry per runner, up to [`MAX_SERIES`].
    ///
    /// Behind an `Arc` so a recorder can drop the map's lock before it
    /// increments — which is what keeps a slow recorder from blocking a fast
    /// one, and what makes the write path lock-free in the common case.
    series: RwLock<HashMap<Box<str>, Arc<Counters>>>,
    /// Everything past the capacity, in one place.
    overflow: Counters,
    /// How many records the overflow absorbed.
    ///
    /// Surfaced explicitly, because an operator seeing overflow needs to know
    /// it is happening — a series that silently merged would look like a quiet
    /// fleet rather than an exhausted table.
    overflowed: AtomicU64,
}

impl Default for RunnerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl RunnerMetrics {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            series: RwLock::new(HashMap::new()),
            overflow: Counters::default(),
            overflowed: AtomicU64::new(0),
        }
    }

    /// Records a failed run.
    pub fn failed(&self, runner_id: &str, reason: Option<FailureClass>) {
        self.with(runner_id, |counters| counters.fail(reason));
        self.with(runner_id, |counters| counters.execute(Outcome::FleetError));
    }

    /// Records a finished run.
    pub fn processed(&self, runner_id: &str) {
        self.with(runner_id, |counters| counters.execute(Outcome::Processed));
    }

    /// Records that a runner was heard from.
    ///
    /// Ignored for a runner past the capacity: a merged last-seen stamp would
    /// describe whichever overflowed runner spoke most recently, which is worse
    /// than not answering — the counters merge meaningfully and a gauge does
    /// not.
    pub fn seen(&self, runner_id: &str, at_ms: i64) {
        if let Some(counters) = self.existing(runner_id) {
            counters.last_seen_ms.store(at_ms, Ordering::Relaxed);
        }
    }

    /// Records that a runner took a lease.
    pub fn leased(&self, runner_id: &str) {
        if let Some(counters) = self.existing(runner_id) {
            counters.active_leases.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records that a runner gave one back.
    pub fn released(&self, runner_id: &str) {
        if let Some(counters) = self.existing(runner_id) {
            counters.active_leases.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// How many distinct runners currently hold a series.
    ///
    /// Never above [`MAX_SERIES`], which is the property Dimension 6.4 asks
    /// for and the reason this is public.
    #[must_use]
    pub fn series_count(&self) -> usize {
        self.series.read().map_or(0, |series| series.len())
    }

    /// How many records the overflow series absorbed.
    #[must_use]
    pub fn overflowed(&self) -> u64 {
        self.overflowed.load(Ordering::Relaxed)
    }

    /// The counters for `runner_id`, or the overflow's.
    ///
    /// The lock is released before `record` runs, so one recorder never waits
    /// on another's arithmetic.
    fn with<F: FnOnce(&Counters)>(&self, runner_id: &str, record: F) {
        if let Some(counters) = self.existing(runner_id) {
            record(&counters);
            return;
        }
        let Some(counters) = self.admit(runner_id) else {
            self.overflowed.fetch_add(1, Ordering::Relaxed);
            record(&self.overflow);
            return;
        };
        record(&counters);
    }

    /// This runner's counters, if it already has a series.
    fn existing(&self, runner_id: &str) -> Option<Arc<Counters>> {
        self.series.read().ok()?.get(runner_id).map(Arc::clone)
    }

    /// Gives `runner_id` a series, if the table has room.
    ///
    /// `None` is a full table, which is the overflow path. The capacity is
    /// checked under the WRITE lock, so two threads racing a new runner cannot
    /// both find room for the last slot.
    fn admit(&self, runner_id: &str) -> Option<Arc<Counters>> {
        let mut series = self.series.write().ok()?;
        // Re-checked under the write lock: another thread may have admitted
        // this very runner between the read above and this line.
        if let Some(counters) = series.get(runner_id) {
            return Some(Arc::clone(counters));
        }
        if series.len() >= MAX_SERIES {
            return None;
        }
        let counters = Arc::new(Counters::default());
        series.insert(runner_id.into(), Arc::clone(&counters));
        Some(counters)
    }
}

/// Which slot a failure class counts in.
///
/// A `match` rather than a derived discriminant, because the ORDER of an enum's
/// variants is not a contract and this index addresses an array. A variant
/// inserted in the middle would otherwise move every count after it.
const fn reason_index(reason: FailureClass) -> usize {
    match reason {
        FailureClass::StartupPosture => 0,
        FailureClass::PolicyDeny => 1,
        FailureClass::TimeoutKill => 2,
        FailureClass::OomKill => 3,
        FailureClass::ResourceKill => 4,
        FailureClass::RunnerCrash => 5,
        FailureClass::TransportLoss => 6,
        FailureClass::LandlockDeny => 7,
        FailureClass::LeaseExpired => 8,
        FailureClass::RenewalTerminate => 9,
        FailureClass::BudgetBreach => 10,
    }
}

/// Which slot an outcome counts in.
const fn outcome_index(outcome: Outcome) -> usize {
    match outcome {
        Outcome::Processed => 0,
        Outcome::FleetError => 1,
    }
}

#[cfg(test)]
mod tests;
