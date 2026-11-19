//! Usage belongs to the current lease and never enters recovered private state.

use af_core::clock::{Clock, SystemClock};
use af_providers::Usage;
use af_wire::usage::{LINES_MAX, RequestLines, RequestUsage};
use std::sync::{
    Arc, Mutex, MutexGuard, PoisonError,
    atomic::{AtomicU64, Ordering},
};

/// The tokens a run has spent so far, readable while it runs, and one line per
/// provider request the daemon has not yet recorded.
///
/// The engine adds each turn's usage. The supervisor sends the unrecorded lines
/// with every lease renewal and forgets the ones the daemon says it recorded;
/// before the report it renews until one body holds the rest, and the report
/// carries those, so the daemon prices every request on its own and a lost
/// response never loses a line. Each total is its own
/// atomic: a read between two adds may see one turn's input without its output.
#[derive(Debug)]
pub struct Meter {
    totals: Totals,
    lines: Mutex<Lines>,
    clock: Arc<dyn Clock>,
}

/// A run's running totals, one atomic per count.
#[derive(Debug, Default)]
struct Totals {
    input: AtomicU64,
    cached_input: AtomicU64,
    cache_write: AtomicU64,
    cache_write_5m: AtomicU64,
    cache_write_1h: AtomicU64,
    output: AtomicU64,
}

impl Totals {
    fn add(&self, usage: Usage) {
        self.input.fetch_add(usage.input, Ordering::Relaxed);
        self.cached_input
            .fetch_add(usage.cached_input, Ordering::Relaxed);
        self.cache_write
            .fetch_add(usage.cache_write, Ordering::Relaxed);
        self.cache_write_5m
            .fetch_add(usage.cache_write_5m, Ordering::Relaxed);
        self.cache_write_1h
            .fetch_add(usage.cache_write_1h, Ordering::Relaxed);
        self.output.fetch_add(usage.output, Ordering::Relaxed);
    }

    fn read(&self) -> Usage {
        Usage {
            input: self.input.load(Ordering::Relaxed),
            cached_input: self.cached_input.load(Ordering::Relaxed),
            cache_write: self.cache_write.load(Ordering::Relaxed),
            cache_write_5m: self.cache_write_5m.load(Ordering::Relaxed),
            cache_write_1h: self.cache_write_1h.load(Ordering::Relaxed),
            output: self.output.load(Ordering::Relaxed),
        }
    }
}

/// The request lines not yet recorded, and the number the next one takes.
#[derive(Debug, Default)]
struct Lines {
    next: u32,
    unrecorded: Vec<RequestUsage>,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock))
    }
}

impl Meter {
    /// A meter stamping each request line with `clock`'s instant.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            totals: Totals::default(),
            lines: Mutex::new(Lines::default()),
            clock,
        }
    }

    /// Adds one turn's tokens, and its request line.
    ///
    /// The line carries each count apart, cache writes by the lifetime the
    /// provider reported, so the daemon prices each at its own rate.
    pub fn add(&self, usage: Usage) {
        self.totals.add(usage);
        let at = self.clock.now();
        let mut lines = self.lines();
        let seq = lines.next;
        // A `model` line's number has the whole `u32`: no other kind shares it.
        lines.next = seq.saturating_add(1);
        lines.unrecorded.push(RequestUsage {
            seq,
            at,
            input_tokens: narrow(usage.input),
            cached_input_tokens: narrow(usage.cached_input),
            cache_write_tokens: narrow(usage.cache_write),
            cache_write_5m_tokens: narrow(usage.cache_write_5m),
            cache_write_1h_tokens: narrow(usage.cache_write_1h),
            output_tokens: narrow(usage.output),
        });
    }

    /// Everything the run has spent so far.
    #[must_use]
    pub fn read(&self) -> Usage {
        self.totals.read()
    }

    /// The oldest request lines the daemon has not recorded, at most one
    /// request body's worth.
    #[must_use]
    pub fn unrecorded(&self) -> RequestLines {
        let lines = self.lines();
        RequestLines(lines.unrecorded.iter().take(LINES_MAX).copied().collect())
    }

    /// How many request lines the daemon has not recorded, which may be more
    /// than one body carries.
    #[must_use]
    pub fn backlog(&self) -> usize {
        self.lines().unrecorded.len()
    }

    /// Forgets every line the daemon recorded, through `seq`.
    pub fn recorded_through(&self, seq: u32) {
        self.lines().unrecorded.retain(|line| line.seq > seq);
    }

    fn lines(&self) -> MutexGuard<'_, Lines> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A count the wire carries in 32 bits, saturated rather than wrapped.
fn narrow(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}
