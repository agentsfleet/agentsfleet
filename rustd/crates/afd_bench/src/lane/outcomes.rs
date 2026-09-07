//! What a driver task did in its window, in vocabulary every lane shares.
//!
//! A lease is issued or missed, a steer is accepted, a poll is refused by a
//! datastore that would not answer: three lanes, one shape. The counts are
//! the same three whatever the verb, and the latency lives in a histogram
//! from the first sample — each task records into its own, constant-time,
//! and the lane folds them at join time. No task ever pushes a `Duration`
//! into a growing buffer inside the loop it is timing.

use core::time::Duration;

use crate::error::Result;
use crate::report::{Latency, ratio};

/// One task's window, or every task's folded together.
#[derive(Debug)]
pub struct Outcomes {
    /// Operations that did what the lane measures: a lease issued, a steer
    /// accepted, a job delivered.
    pub successes: u64,
    /// Operations that ran and found nothing to do.
    pub misses: u64,
    /// Operations the path refused: a datastore that would not answer.
    pub failures: u64,
    /// How long each success and miss took.
    pub latency: Latency,
}

impl Outcomes {
    /// An empty window.
    ///
    /// # Errors
    ///
    /// [`crate::Error::LatencyUnavailable`] when the histogram will not build.
    pub fn new() -> Result<Self> {
        Ok(Self {
            successes: 0,
            misses: 0,
            failures: 0,
            latency: Latency::new()?,
        })
    }

    /// Record an operation that succeeded.
    ///
    /// # Errors
    ///
    /// [`crate::Error::LatencyUnrecordable`] for a duration past the histogram.
    pub fn succeeded(&mut self, took: Duration) -> Result<()> {
        self.successes += 1;
        self.latency.record(took)
    }

    /// Record an operation that found nothing.
    ///
    /// # Errors
    ///
    /// [`crate::Error::LatencyUnrecordable`] for a duration past the histogram.
    pub fn missed(&mut self, took: Duration) -> Result<()> {
        self.misses += 1;
        self.latency.record(took)
    }

    /// Record an operation the path refused.
    pub const fn failed(&mut self) {
        self.failures += 1;
    }

    /// Fold another task's window into this one.
    ///
    /// # Errors
    ///
    /// [`crate::Error::LatencyUnmergeable`] when the histograms will not add.
    pub fn absorb(&mut self, other: &Self) -> Result<()> {
        self.successes += other.successes;
        self.misses += other.misses;
        self.failures += other.failures;
        self.latency.merge(&other.latency)
    }

    /// Every operation that ran to an answer, success or miss.
    #[must_use]
    pub const fn attempts(&self) -> u64 {
        self.successes + self.misses
    }

    /// The fraction of answered operations that found nothing.
    ///
    /// Measured from OUTSIDE the path, which does not publish a per-operation
    /// reason: under contention a miss is dominated by work another task took
    /// first, so this is an upper bound on wasted claims, not a count.
    #[must_use]
    pub fn wasted_fraction(&self) -> f64 {
        ratio(self.misses, self.attempts())
    }

    /// The fraction of everything tried that the path refused.
    #[must_use]
    pub fn failure_fraction(&self) -> f64 {
        ratio(self.failures, self.attempts() + self.failures)
    }
}

#[cfg(test)]
mod tests;
