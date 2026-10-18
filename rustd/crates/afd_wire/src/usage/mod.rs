//! What one provider request used, as the runner reports it.
//!
//! The daemon prices each request on its own (billing §11): a context tier
//! prices the WHOLE request, so a run's cumulative totals cannot say which
//! requests crossed a tier, and a price can change at a date mid-run. One line
//! per request carries what that pricing needs and nothing else; the daemon
//! turns it into money. A line with no tokens is a runtime-only slice: a
//! renewal that saw no provider request still reports the time it ran.
//!
//! `seq` numbers a work's lines from zero. The daemon stores each `(work, seq)`
//! once, so a retried renewal or report re-sends lines harmlessly.

use garde::Validate;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// The most lines one renewal or report may carry.
///
/// A renewal is sent every few minutes and a run makes far fewer provider
/// requests than this in that time; the cap bounds what one request body can
/// make the daemon price.
pub const LINES_MAX: usize = 512;

/// The longest runtime one line may claim, in milliseconds: one hour.
///
/// A line is cut at least at every renewal, so a longer claim is a malformed
/// report rather than a long run.
pub const RUNTIME_MS_MAX: u32 = 3_600_000;

/// One provider request's token counts, or a runtime-only slice.
///
/// Each token is counted once: cached reads are not also input, cache writes
/// are not also input, and reasoning is part of `output_tokens` exactly once.
/// Cache writes split by the lifetime the provider reported; a write with no
/// reported lifetime is `cache_write_tokens`, never folded into either
/// lifetime.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UsageLine {
    /// This line's number within its work, from zero.
    #[garde(skip)]
    pub seq: u32,
    /// When the request was made, RFC 3339. The daemon clamps it to the work's
    /// own window before pricing.
    #[garde(skip)]
    #[serde(with = "crate::instant")]
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = DateTime))]
    pub at: Timestamp,
    /// Uncached prompt tokens.
    #[garde(skip)]
    pub input_tokens: u32,
    /// Prompt tokens read from the provider's cache.
    #[garde(skip)]
    pub cached_input_tokens: u32,
    /// Cache writes the provider reported no lifetime for.
    #[garde(skip)]
    pub cache_write_tokens: u32,
    /// Cache writes with a five-minute lifetime.
    #[garde(skip)]
    pub cache_write_5m_tokens: u32,
    /// Cache writes with a one-hour lifetime.
    #[garde(skip)]
    pub cache_write_1h_tokens: u32,
    /// Completion tokens, reasoning included once.
    #[garde(skip)]
    pub output_tokens: u32,
    /// Run time this line accounts for, in milliseconds.
    #[garde(range(max = RUNTIME_MS_MAX))]
    pub runtime_ms: u32,
}

impl UsageLine {
    /// The request's whole prompt: what a context tier is measured against.
    ///
    /// Uncached input, cache reads and every cache write together, because a
    /// provider's tier threshold counts the prompt it received, however much of
    /// it was served from cache.
    #[must_use]
    pub const fn whole_prompt(&self) -> u64 {
        self.input_tokens as u64
            + self.cached_input_tokens as u64
            + self.cache_write_tokens as u64
            + self.cache_write_5m_tokens as u64
            + self.cache_write_1h_tokens as u64
    }
}

/// The lines one renewal or report carries: at least one, at most [`LINES_MAX`].
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(transparent)]
#[garde(transparent)]
pub struct UsageLines(#[garde(length(min = 1, max = LINES_MAX), dive)] pub Vec<UsageLine>);

#[cfg(test)]
mod tests;
