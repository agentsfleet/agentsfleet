//! What a fleet's recall costs, and what this process is holding.

use std::sync::Arc;

use opentelemetry::metrics::Counter;

use crate::error::Result;
use crate::metrics::declared::memory as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::observed::Observed;
use crate::producers::{GaugeSources, installed};

/// Entries the last hydration window carried.
///
/// Published where the window is computed, because that is the only place the
/// number exists — a callback cannot re-derive it without redoing the
/// selection, under the pipeline lock, against rows it would have to fetch.
static WINDOW_ENTRIES: Observed = Observed::new();

/// Publishes what a hydration window carried, and what it left behind.
///
/// One call for the gauge and both counters, because a window that reported
/// its size without its drops would say a run was seeded and not that anything
/// was withheld from it.
pub fn hydration_window(kept: u64, dropped_entries: u64, dropped_bytes: u64) {
    WINDOW_ENTRIES.publish(kept);
    hydration_dropped(dropped_entries, dropped_bytes);
}

/// The instruments the memory paths record through.
#[derive(Debug)]
pub struct Handles {
    captured: Counter<u64>,
    push_failures: Counter<u64>,
    dropped_entries: Counter<u64>,
    dropped_bytes: Counter<u64>,
    cap_evictions: Counter<u64>,
    truncated: Counter<u64>,
    skipped: Counter<u64>,
    search_zero_hits: Counter<u64>,
}

impl Handles {
    /// Claims every instrument this domain records through.
    ///
    /// # Errors
    ///
    /// Whatever [`Instruments`] refuses — see [`super::install`].
    pub(super) fn claim(instruments: &Instruments, sources: &GaugeSources) -> Result<Self> {
        let handles = Self {
            captured: instruments.counter_u64(&declared::MEMORY_ENTRIES_CAPTURED_TOTAL)?,
            push_failures: instruments.counter_u64(&declared::MEMORY_PUSH_FAILURES_TOTAL)?,
            dropped_entries: instruments
                .counter_u64(&declared::MEMORY_HYDRATION_DROPPED_ENTRIES_TOTAL)?,
            dropped_bytes: instruments
                .counter_u64(&declared::MEMORY_HYDRATION_DROPPED_BYTES_TOTAL)?,
            cap_evictions: instruments.counter_u64(&declared::MEMORY_CAP_EVICTIONS_TOTAL)?,
            truncated: instruments.counter_u64(&declared::MEMORY_CAPTURE_TRUNCATED_TOTAL)?,
            skipped: instruments.counter_u64(&declared::MEMORY_CAPTURE_SKIPPED_TOTAL)?,
            search_zero_hits: instruments.counter_u64(&declared::MEMORY_SEARCH_ZERO_HITS_TOTAL)?,
        };

        instruments.gauge_u64(&declared::MEMORY_HYDRATION_WINDOW_ENTRIES, || {
            WINDOW_ENTRIES
                .load()
                .into_iter()
                .map(Reading::unlabelled)
                .collect()
        })?;

        let resident = Arc::clone(&sources.resident_memory);
        instruments.gauge_u64(&declared::PROCESS_RESIDENT_MEMORY_BYTES, move || {
            resident().into_iter().map(Reading::unlabelled).collect()
        })?;

        Ok(handles)
    }
}

/// Records entries a fleet committed to memory.
pub fn captured(entries: u64) {
    if let Some(producers) = installed() {
        producers.memory.captured.add(entries, &[]);
    }
}

/// Records a capture the datastore would not take.
pub fn push_failed() {
    if let Some(producers) = installed() {
        producers.memory.push_failures.add(1, &[]);
    }
}

/// Records what a hydration window would not carry.
///
/// Both counters move together because the pair is the number worth reading: a
/// window that dropped one large entry and one that dropped forty small ones
/// are different problems, and either count alone hides which.
pub fn hydration_dropped(entries: u64, bytes: u64) {
    if let Some(producers) = installed() {
        producers.memory.dropped_entries.add(entries, &[]);
        producers.memory.dropped_bytes.add(bytes, &[]);
    }
}

/// Records entries a fleet's own ceiling evicted.
pub fn cap_evicted(entries: u64) {
    if let Some(producers) = installed() {
        producers.memory.cap_evictions.add(entries, &[]);
    }
}

/// Records a capture stored shorter than it arrived.
pub fn capture_truncated() {
    if let Some(producers) = installed() {
        producers.memory.truncated.add(1, &[]);
    }
}

/// Records a capture this daemon declined to store at all.
pub fn capture_skipped() {
    if let Some(producers) = installed() {
        producers.memory.skipped.add(1, &[]);
    }
}

/// Records a search that matched nothing.
///
/// Worth its own family because it is the signal that a fleet's memory is not
/// being reached: a search path can be perfectly healthy and useless.
pub fn search_found_nothing() {
    if let Some(producers) = installed() {
        producers.memory.search_zero_hits.add(1, &[]);
    }
}
