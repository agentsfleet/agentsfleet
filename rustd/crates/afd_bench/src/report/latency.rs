//! The latency distribution a lane reports, recorded as it goes.
//!
//! # Why a histogram and not a list of samples
//!
//! A lane at the rig profile issues up to a million operations. Keeping every
//! duration would make the sample buffer the largest thing in the process and
//! put a sort between the last operation and the result file. `HdrHistogram`
//! records in constant time into log-linear buckets, so a run holding a
//! million samples costs what one holding a thousand costs, and the quantile
//! is a lookup rather than a pass over the data.
//!
//! The cost is that a quantile is accurate to [`SIGNIFICANT_FIGURES`] rather
//! than exact. At three figures that is a tenth of a percent, which is smaller
//! than the run-to-run variance of anything measured against a real datastore.

use core::time::Duration;

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Precision every reported quantile carries.
///
/// Three figures resolves 1.00 ms from 1.01 ms, which is finer than any
/// decision these lanes inform, and costs a bounded number of buckets per
/// order of magnitude rather than growing with the sample count.
pub const SIGNIFICANT_FIGURES: u8 = 3;

/// Microseconds in a millisecond, for reporting a recorded value.
const MICROS_PER_MILLI: f64 = 1_000.0;

/// The quantile a lane reports as its headline tail latency.
pub const P95: f64 = 0.95;

/// The quantile a lane reports beside it, where the tail actually lives.
pub const P99: f64 = 0.99;

/// Every operation a lane timed, as a distribution.
#[derive(Debug, Clone)]
pub struct Latency {
    histogram: Histogram<u64>,
}

/// One non-empty histogram bucket retained as raw evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistogramBucket {
    /// Highest equivalent microsecond value represented by the bucket.
    pub value_micros: u64,
    /// Observations landing in this bucket.
    pub count: u64,
}

impl Latency {
    /// An empty distribution, ready to record.
    ///
    /// # Errors
    ///
    /// [`Error::LatencyUnavailable`] when the histogram cannot be built, which
    /// means the precision above is not one `HdrHistogram` accepts.
    pub fn new() -> Result<Self> {
        Histogram::new(SIGNIFICANT_FIGURES)
            .map(|histogram| Self { histogram })
            .map_err(|source| Error::LatencyUnavailable { source })
    }

    /// Record one operation.
    ///
    /// # Errors
    ///
    /// [`Error::LatencyUnrecordable`] when the value is past what the
    /// histogram can hold even after resizing.
    pub fn record(&mut self, elapsed: Duration) -> Result<()> {
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        self.histogram
            .record(micros)
            .map_err(|source| Error::LatencyUnrecordable { source })
    }

    /// Fold another distribution into this one.
    ///
    /// Each driver task records into its own histogram so nothing is shared
    /// inside a timed loop; the lane merges them once at join time. Bucketed
    /// values add exactly, so the merged quantiles are what one histogram
    /// would have reported had every task recorded into it.
    ///
    /// # Errors
    ///
    /// [`Error::LatencyUnmergeable`] when the other histogram holds a value
    /// this one cannot, which cannot happen for two built by [`Latency::new`].
    pub fn merge(&mut self, other: &Self) -> Result<()> {
        self.histogram
            .add(&other.histogram)
            .map_err(|source| Error::LatencyUnmergeable { source })
    }

    /// How many operations were recorded.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.histogram.len()
    }

    /// Whether anything was recorded at all.
    ///
    /// A lane that recorded nothing has no rate to report, and reporting a
    /// zero would be a measurement of something that never ran.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// The value at a quantile, in milliseconds.
    #[must_use]
    pub fn quantile_ms(&self, quantile: f64) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a microsecond count large enough to lose precision in f64 is \
                      a latency of a hundred years"
        )]
        {
            self.histogram.value_at_quantile(quantile) as f64 / MICROS_PER_MILLI
        }
    }

    /// The slowest operation recorded, in milliseconds.
    #[must_use]
    pub fn max_ms(&self) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "see quantile_ms: the magnitude cannot arise from a real duration"
        )]
        {
            self.histogram.max() as f64 / MICROS_PER_MILLI
        }
    }

    /// Every non-empty bucket needed to reconstruct this distribution.
    #[must_use]
    pub fn buckets(&self) -> Vec<HistogramBucket> {
        self.histogram
            .iter_recorded()
            .map(|entry| HistogramBucket {
                value_micros: entry.value_iterated_to(),
                count: entry.count_since_last_iteration(),
            })
            .collect()
    }

    /// Rebuild a distribution from archived buckets.
    ///
    /// # Errors
    ///
    /// Refuses an invalid histogram shape or a bucket outside its range.
    pub fn from_buckets(buckets: &[HistogramBucket]) -> Result<Self> {
        let mut latency = Self::new()?;
        for bucket in buckets {
            latency
                .histogram
                .record_n(bucket.value_micros, bucket.count)
                .map_err(|source| Error::LatencyUnrecordable { source })?;
        }
        Ok(latency)
    }
}

#[cfg(test)]
mod tests;
