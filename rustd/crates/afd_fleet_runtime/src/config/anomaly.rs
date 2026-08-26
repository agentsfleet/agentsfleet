//! The repetition shapes that end a run, and the bounds an author may set.
//!
//! Split from [`super::gates`] on the domain seam rather than on line count: an
//! approval rule asks a HUMAN and an anomaly rule asks nobody. They share a
//! container in the stored document, and nothing else — different fields,
//! different failure, different moment in the pass. `approval_gate_anomaly.zig`
//! is its own module upstream for the same reason.

use std::num::NonZeroU32;

use crate::config::raw;
use crate::error::{Error, Result};

/// The key naming a rule's repeat count.
const THRESHOLD_COUNT: &str = "threshold_count";
/// The key naming its window.
const THRESHOLD_WINDOW_S: &str = "threshold_window_s";
/// The key naming its pattern.
const PATTERN: &str = "pattern";

/// Most repeats a rule may require.
const MAX_ANOMALY_REPEATS: u32 = 10_000;
/// Longest window those repeats may be counted over.
const MAX_ANOMALY_WINDOW_S: u32 = 86_400;

/// Why a threshold was refused.
const REASON_ZERO: &str = "zero would trip on the first action and never stop";
/// See [`REASON_ZERO`].
const REASON_ABOVE_CAP: &str = "it is above the cap";

/// A repetition shape a rule watches for.
pub type Pattern = raw::Pattern;

/// A repetition that ends the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnomalyRule {
    /// Which repetition shape trips it.
    pub pattern: Pattern,
    /// How many repeats trip it.
    pub repeats: NonZeroU32,
    /// The window those repeats are counted in, in seconds.
    pub window_s: NonZeroU32,
}

impl TryFrom<raw::AnomalyRule> for AnomalyRule {
    type Error = Error;

    fn try_from(authored: raw::AnomalyRule) -> Result<Self> {
        Ok(Self {
            pattern: authored.pattern.ok_or_else(|| Error::missing(PATTERN))?,
            repeats: threshold(
                THRESHOLD_COUNT,
                authored.threshold_count,
                MAX_ANOMALY_REPEATS,
            )?,
            window_s: threshold(
                THRESHOLD_WINDOW_S,
                authored.threshold_window_s,
                MAX_ANOMALY_WINDOW_S,
            )?,
        })
    }
}

/// Checks one threshold and proves it non-zero.
///
/// `NonZeroU32` rather than a checked `u32`, so the proof travels WITH the
/// value: a counter comparison downstream cannot be handed a zero, because
/// there is no zero to hand it.
///
/// # Errors
/// [`Error::MissingRequiredField`] when absent, [`Error::InvalidThreshold`]
/// when zero or above `cap`.
fn threshold(field: &'static str, authored: Option<u32>, cap: u32) -> Result<NonZeroU32> {
    let value = authored.ok_or_else(|| Error::missing(field))?;

    if value > cap {
        return Err(Error::InvalidThreshold {
            field,
            reason: REASON_ABOVE_CAP,
        });
    }

    NonZeroU32::new(value).ok_or(Error::InvalidThreshold {
        field,
        reason: REASON_ZERO,
    })
}
