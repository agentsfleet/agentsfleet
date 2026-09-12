//! Raw observations and the deterministic calculation each reported number uses.

use core::time::Duration;

use serde::{Deserialize, Serialize};

use super::latency::{HistogramBucket, Latency};
use crate::error::{Error, Result};

const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
const NANOS_PER_MILLI: f64 = 1_000_000.0;
const QUANTILE_SCALE: f64 = 1_000_000.0;

/// A frozen input plus the formula that turns it into one report value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Calculation {
    /// An observed integer count.
    Count {
        /// Observed count.
        value: u64,
    },
    /// A monotonic counter delta.
    Difference {
        /// Counter before the measured operation.
        before: u64,
        /// Counter after the measured operation.
        after: u64,
    },
    /// A numerator divided by its denominator.
    Ratio {
        /// Observed numerator.
        numerator: u64,
        /// Observed denominator.
        denominator: u64,
    },
    /// A count divided by a measured wall-clock span.
    Rate {
        /// Operations observed.
        count: u64,
        /// Exact measured span in nanoseconds.
        elapsed_nanos: u64,
    },
    /// A boolean represented as zero or one.
    Flag {
        /// Observed truth value.
        value: bool,
    },
    /// A quantile from the raw histogram buckets.
    HistogramQuantile {
        /// Quantile on a fixed million-part scale.
        quantile_millionths: u32,
        /// Non-empty raw histogram buckets.
        buckets: Vec<HistogramBucket>,
    },
    /// The largest value in the raw histogram buckets.
    HistogramMaximum {
        /// Non-empty raw histogram buckets.
        buckets: Vec<HistogramBucket>,
    },
    /// The upper middle of an ordered duration sample, matching the driver.
    MedianDuration {
        /// Every sampled duration in nanoseconds.
        samples_nanos: Vec<u64>,
    },
    /// A labelled millisecond value parsed from raw datastore output.
    ParsedMillis {
        /// Exact prefix identifying the selected line.
        label: String,
        /// Raw datastore output lines.
        lines: Vec<String>,
    },
}

impl Calculation {
    /// Evaluate from the frozen inputs, without consulting a stored result.
    ///
    /// # Errors
    ///
    /// Refuses malformed histogram or labelled-output observations.
    pub fn evaluate(&self) -> Result<f64> {
        match self {
            Self::Count { value } => Ok(count(*value)),
            Self::Difference { before, after } => Ok(count(after.saturating_sub(*before))),
            Self::Ratio {
                numerator,
                denominator,
            } => Ok(ratio(*numerator, *denominator)),
            Self::Rate {
                count: observed,
                elapsed_nanos,
            } => Ok(rate(*observed, *elapsed_nanos)),
            Self::Flag { value } => Ok(if *value { 1.0 } else { 0.0 }),
            Self::HistogramQuantile {
                quantile_millionths,
                buckets,
            } => Latency::from_buckets(buckets).map(|histogram| {
                histogram.quantile_ms(f64::from(*quantile_millionths) / QUANTILE_SCALE)
            }),
            Self::HistogramMaximum { buckets } => {
                Latency::from_buckets(buckets).map(|histogram| histogram.max_ms())
            }
            Self::MedianDuration { samples_nanos } => median(samples_nanos)
                .map(|nanos| count(nanos) / NANOS_PER_MILLI)
                .ok_or_else(|| invalid("duration observation is empty")),
            Self::ParsedMillis { label, lines } => lines
                .iter()
                .find_map(|line| line.trim().strip_prefix(label))
                .and_then(|raw| raw.trim_end_matches(" ms").parse().ok())
                .ok_or_else(|| invalid("labelled millisecond observation is unreadable")),
        }
    }

    pub(crate) fn count(value: u64) -> Self {
        Self::Count { value }
    }

    pub(crate) fn ratio(numerator: u64, denominator: u64) -> Self {
        Self::Ratio {
            numerator,
            denominator,
        }
    }

    pub(crate) fn ratio_value(numerator: u64, denominator: u64) -> f64 {
        ratio(numerator, denominator)
    }

    pub(crate) fn rate(count: u64, elapsed: Duration) -> Self {
        Self::Rate {
            count,
            elapsed_nanos: nanos(elapsed),
        }
    }

    pub(crate) fn rate_value(count: u64, elapsed: Duration) -> f64 {
        rate(count, nanos(elapsed))
    }

    pub(crate) fn duration_ratio_value(numerator: Duration, denominator: Duration) -> f64 {
        ratio(nanos(numerator), nanos(denominator))
    }

    pub(crate) fn flag(value: bool) -> Self {
        Self::Flag { value }
    }

    pub(crate) fn quantile(latency: &Latency, quantile_millionths: u32) -> Self {
        Self::HistogramQuantile {
            quantile_millionths,
            buckets: latency.buckets(),
        }
    }

    pub(crate) fn maximum(latency: &Latency) -> Self {
        Self::HistogramMaximum {
            buckets: latency.buckets(),
        }
    }

    pub(crate) fn median(samples: &[Duration]) -> Self {
        Self::MedianDuration {
            samples_nanos: samples.iter().copied().map(nanos).collect(),
        }
    }

    pub(crate) fn median_value(samples: &[Duration]) -> Option<f64> {
        let samples_nanos = samples.iter().copied().map(nanos).collect::<Vec<_>>();
        median(&samples_nanos).map(|value| count(value) / NANOS_PER_MILLI)
    }
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn median(samples: &[u64]) -> Option<u64> {
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    ordered.get(ordered.len() / 2).copied()
}

fn rate(value: u64, elapsed_nanos: u64) -> f64 {
    if elapsed_nanos == 0 {
        return 0.0;
    }
    count(value) * NANOS_PER_SECOND / count(elapsed_nanos)
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    count(numerator) / count(denominator)
}

fn count(value: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count past f64's exact range cannot arise in one benchmark run"
    )]
    {
        value as f64
    }
}

fn invalid(detail: &str) -> Error {
    Error::EvidenceInvalid(detail.to_owned())
}
