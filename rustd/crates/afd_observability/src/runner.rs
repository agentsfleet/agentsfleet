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
//! # The overflow spelling, settled (M181 §3)
//!
//! This module used to defer the overflow series' NAME to the milestone that
//! configures the metrics pipeline. That milestone is M181, and the answer is
//! that the two candidate spellings are not alternatives — they mean different
//! things and both exist.
//!
//! [`OVERFLOW_RUNNER`] (`_other`) is OURS. It is a domain decision made in
//! front of the instrument: past [`MAX_SERIES`] admitted runners, the rest are
//! attributed together on purpose, and the total stays correct. An operator
//! seeing it is seeing a deployment larger than the slot table, which is
//! information, not a fault.
//!
//! `otel.metric.overflow` is the SDK's, spec-fixed, and set when the SDK's own
//! per-stream cardinality cap is hit. It is a BACKSTOP behind our admission and
//! it must never fire — if it does, something wrote an attribute the typed
//! layer was supposed to make unwritable. Spelling that as `_other` would
//! disguise a bug as a capacity notice, which is why the two stay distinct and
//! why a negative test asserts zero data points ever carry the SDK's marker.
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

/// The `runner_id` value every runner past the slot table is attributed under.
///
/// The Zig daemon's spelling, kept byte-exact: every dashboard and alert that
/// reads this label reads it on both sides of the swap, and a renamed overflow
/// bucket is a panel that silently stops matching.
///
/// Deliberately NOT `otel.metric.overflow` — see the module note. This is a
/// bounded-attribution decision; that is a bug indicator.
pub const OVERFLOW_RUNNER: &str = "_other";

/// The attribute key the SDK sets when its OWN cardinality cap is hit.
///
/// Named here so a test can assert it never appears. Nothing in this crate
/// writes it; the SDK does, and only when admission upstream has already
/// failed to do its job.
pub const SDK_OVERFLOW_MARKER: &str = "otel.metric.overflow";

/// How many distinct runners get their own series.
///
/// The Zig's, and the number is not the point — the bound is. Four thousand is
/// far past any real deployment's host count and far below anything that
/// threatens memory.
pub const MAX_SERIES: usize = 4096;

/// Milliseconds per second, for the one conversion this module performs.
const MILLIS_PER_SECOND: u64 = 1_000;

/// The stamp a runner nothing has heard from carries.
const NEVER_HEARD_FROM: i64 = 0;

/// The readings one series holds.
///
/// Atomics rather than a lock, so publishing never blocks another publisher —
/// the map's lock is taken for LOOKUP, and released before anything is
/// written.
///
/// # Why there are no counts here
///
/// There were, and they were a second record of what the export already
/// carries: a per-reason failure array and a per-outcome execution array,
/// summed into a snapshot the exporter read. The instrument layer records
/// those directly now, so keeping the arrays would mean two counts of one
/// event that could only ever drift apart. What is left is the state a
/// COLLECTION CALLBACK has to read and cannot compute — when a runner was last
/// heard from, and what it is believed to hold.
#[derive(Debug, Default)]
struct Counters {
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

/// The per-runner metric families this process holds.
///
/// One value the daemon owns, rather than the Zig's process-global mutable
/// arrays: a test can build its own and drive it to the cardinality edge
/// without touching what any other test is counting.
#[derive(Debug)]
pub struct RunnerMetrics {
    /// One entry per runner, up to [`MAX_SERIES`].
    ///
    /// Behind an `Arc` so a publisher can drop the map's lock before it
    /// writes — which is what keeps a slow publisher from blocking a fast one,
    /// and what makes the write path lock-free in the common case.
    series: RwLock<HashMap<Box<str>, Arc<Counters>>>,
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
            overflowed: AtomicU64::new(0),
        }
    }

    /// Admits `runner_id` if the table has room, answering the label its
    /// measurements are recorded under.
    ///
    /// This is where the cardinality bound is actually applied: a runner past
    /// the table's capacity is attributed to [`OVERFLOW_RUNNER`] and counted as
    /// having overflowed. A caller that recorded against the raw identifier
    /// instead would make the ceiling a suggestion, which is why the label is
    /// answered by the table rather than chosen by the caller.
    pub fn admit<'id>(&self, runner_id: &'id str) -> &'id str {
        if self.existing(runner_id).is_some() {
            return runner_id;
        }
        if self.admit_new(runner_id).is_some() {
            return runner_id;
        }
        self.overflowed.fetch_add(1, Ordering::Relaxed);
        OVERFLOW_RUNNER
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

    /// The `runner_id` label value this runner's measurements are recorded
    /// under: its own identifier, or [`OVERFLOW_RUNNER`] once the table is full.
    ///
    /// This is the admission decision made READABLE. The bound already existed
    /// — `with` silently routes an unadmitted runner to the overflow counters —
    /// but the label that decision produces was never exposed, so the export
    /// layer had no way to ask what a measurement should be attributed to
    /// without duplicating the rule.
    ///
    /// Answering the identifier is not a promise it is admitted YET: a runner
    /// with room is admitted by its first record, not by being asked about.
    /// What is promised is the label a record made now would carry.
    #[must_use]
    pub fn label_for<'id>(&self, runner_id: &'id str) -> &'id str {
        if self.existing(runner_id).is_some() || self.has_room() {
            return runner_id;
        }
        OVERFLOW_RUNNER
    }

    /// Whether the table could still admit a runner it has not seen.
    fn has_room(&self) -> bool {
        self.series
            .read()
            .is_ok_and(|series| series.len() < MAX_SERIES)
    }

    /// What every admitted runner was last heard from at, in seconds.
    ///
    /// Milliseconds are the storage and seconds are the wire, because the
    /// census declares this family in seconds — converting at the callback is
    /// what keeps one conversion in the process rather than one per reader.
    ///
    /// A runner that has never been heard from publishes NO reading: a zero
    /// there would read as 1970, which every dashboard would draw as the
    /// oldest host in the fleet.
    #[must_use]
    pub fn last_seen_readings(&self) -> Vec<crate::metrics::instrument::Reading> {
        self.readings(|counters| {
            let stamp = counters.last_seen_ms.load(Ordering::Relaxed);
            // Zero is the ABSENCE sentinel, not a time. A runner admitted and
            // never heard from would otherwise publish the epoch, which every
            // dashboard draws as the oldest host in the fleet — and no runner
            // reports from 1970, so nothing real is lost by refusing it. The
            // delivery span refuses a pre-epoch start for the same reason.
            (stamp > NEVER_HEARD_FROM)
                .then(|| u64::try_from(stamp).ok().map(|ms| ms / MILLIS_PER_SECOND))
                .flatten()
        })
    }

    /// How many leases each admitted runner is believed to hold.
    ///
    /// Best effort, and knowingly: a lease abandoned by a dead runner expires
    /// by the clock with no report to decrement against, so a crashed host's
    /// reading stays high until the process restarts. The liveness sweeper's
    /// row is the authority; this is a hint beside it.
    #[must_use]
    pub fn active_lease_readings(&self) -> Vec<crate::metrics::instrument::Reading> {
        self.readings(|counters| u64::try_from(counters.active_leases.load(Ordering::Relaxed)).ok())
    }

    /// One reading per admitted runner, for those that have one.
    fn readings<F>(&self, read: F) -> Vec<crate::metrics::instrument::Reading>
    where
        F: Fn(&Counters) -> Option<u64>,
    {
        let Ok(series) = self.series.read() else {
            return Vec::new();
        };
        series
            .iter()
            .filter_map(|(runner_id, counters)| {
                read(counters).map(|value| crate::metrics::instrument::Reading {
                    attributes: vec![opentelemetry::KeyValue::new(
                        crate::semconv::LABEL_RUNNER_ID,
                        runner_id.to_string(),
                    )],
                    value,
                })
            })
            .collect()
    }

    /// How many records the overflow series absorbed.
    #[must_use]
    pub fn overflowed(&self) -> u64 {
        self.overflowed.load(Ordering::Relaxed)
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
    fn admit_new(&self, runner_id: &str) -> Option<Arc<Counters>> {
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

#[cfg(test)]
mod tests;
