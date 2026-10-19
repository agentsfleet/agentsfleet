//! What one provider request used, as the runner reports it, and the priced
//! line the daemon makes of it.
//!
//! The daemon prices each request on its own (billing §11): a context tier
//! prices the WHOLE request, so a run's cumulative totals cannot say which
//! requests crossed a tier, and a price can change at a date mid-run. The
//! runner reports one [`RequestUsage`] per provider request; the daemon turns
//! each into a [`UsageLine`] and adds its own runtime-only line, measured on
//! its own clock, for the time the run held its lease.
//!
//! Lines are numbered per work and stored once per `(work, seq)`, so a retried
//! renewal or report re-sends lines harmlessly. The two writers number from
//! disjoint ranges: the runner below [`DAEMON_SEQ_BASE`], the daemon from it,
//! so a runner line and a daemon runtime line can never claim one number.

use std::time::Duration;

use garde::Validate;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// The most request lines one renewal or report may carry.
///
/// A renewal is sent every few minutes and a run makes far fewer provider
/// requests than this in that time; the cap bounds what one request body can
/// make the daemon price.
pub const LINES_MAX: usize = 512;

/// The longest runtime one line may claim: one hour.
///
/// The daemon cuts a runtime line at every renewal, so a longer claim is a
/// malformed line rather than a long run.
pub const RUNTIME_MAX: Duration = Duration::from_hours(1);

/// [`RUNTIME_MAX`] as the bound on `runtime_ms`, the wire's span field: the one
/// place it becomes milliseconds, because `garde` takes an integer.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the const assertion inside this block proves the value sits \
              inside u32 before the cast runs, so a bound edited past the \
              wire's range fails the build rather than truncating"
)]
pub const RUNTIME_MS_MAX: u32 = {
    const _: () = assert!(
        RUNTIME_MAX.as_millis() <= u32::MAX as u128,
        "the runtime bound must be a span the wire can carry"
    );
    RUNTIME_MAX.as_millis() as u32
};

/// The first line number the daemon's own runtime lines take.
///
/// The runner numbers its request lines from zero up to [`RUNNER_SEQ_MAX`]; the
/// daemon numbers its runtime lines from here, so the two never collide.
pub const DAEMON_SEQ_BASE: u32 = 1 << 31;

/// The highest number a runner's request line may take.
pub const RUNNER_SEQ_MAX: u32 = DAEMON_SEQ_BASE - 1;

/// One provider request's token counts, as the runner reports it.
///
/// Each token is counted once: cached reads are not also input, cache writes
/// are not also input, and reasoning is part of `output_tokens` exactly once.
/// Cache writes split by the lifetime the provider reported; a write with no
/// reported lifetime is `cache_write_tokens`, never folded into either
/// lifetime. Runtime is not here: the daemon measures it.
//
// Every count is a `u32`, published as `int64` with a maximum of `u32::MAX`:
// utoipa's default `int32` would tell a generated client the top half of the
// range does not exist. utoipa takes only a literal for the bound, so
// `tests::the_published_token_counts_span_u32` keeps it equal to `u32::MAX`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RequestUsage {
    /// This request's number within its run, from zero.
    #[garde(range(max = RUNNER_SEQ_MAX))]
    pub seq: u32,
    /// When the request was made, RFC 3339. The daemon clamps it to the work's
    /// own window before pricing.
    #[garde(skip)]
    #[serde(with = "crate::instant")]
    #[cfg_attr(feature = "openapi", schema(value_type = String, format = DateTime))]
    pub at: Timestamp,
    /// Uncached prompt tokens.
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(format = Int64, maximum = 4_294_967_295_u64))]
    pub input_tokens: u32,
    /// Prompt tokens read from the provider's cache.
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(format = Int64, maximum = 4_294_967_295_u64))]
    pub cached_input_tokens: u32,
    /// Cache writes the provider reported no lifetime for.
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(format = Int64, maximum = 4_294_967_295_u64))]
    pub cache_write_tokens: u32,
    /// Cache writes with a five-minute lifetime.
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(format = Int64, maximum = 4_294_967_295_u64))]
    pub cache_write_5m_tokens: u32,
    /// Cache writes with a one-hour lifetime.
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(format = Int64, maximum = 4_294_967_295_u64))]
    pub cache_write_1h_tokens: u32,
    /// Completion tokens, reasoning included once.
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(format = Int64, maximum = 4_294_967_295_u64))]
    pub output_tokens: u32,
}

impl RequestUsage {
    /// This request as a usage line that claims no runtime of its own.
    #[must_use]
    pub const fn line(self) -> UsageLine {
        UsageLine {
            seq: self.seq,
            at: self.at,
            input_tokens: self.input_tokens,
            cached_input_tokens: self.cached_input_tokens,
            cache_write_tokens: self.cache_write_tokens,
            cache_write_5m_tokens: self.cache_write_5m_tokens,
            cache_write_1h_tokens: self.cache_write_1h_tokens,
            output_tokens: self.output_tokens,
            runtime_ms: 0,
        }
    }
}

/// The request lines one renewal or report carries: none to [`LINES_MAX`].
///
/// A renewal that saw no provider request since the last one sends none; the
/// daemon still logs the time it ran.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(transparent)]
#[garde(transparent)]
pub struct RequestLines(#[garde(length(max = LINES_MAX), dive)] pub Vec<RequestUsage>);

/// One priced unit of work: a provider request, or a runtime-only slice.
///
/// What the biller charges and `billing.usage_lines` stores. A request line
/// carries no runtime; a runtime line carries no tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UsageLine {
    /// This line's number within its work.
    #[garde(skip)]
    pub seq: u32,
    /// When the request was made or the slice was measured, RFC 3339.
    #[garde(skip)]
    #[serde(with = "crate::instant")]
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
    /// A runtime-only line: `runtime_ms` of run time measured at `at`.
    #[must_use]
    pub const fn runtime(seq: u32, at: Timestamp, runtime_ms: u32) -> Self {
        Self {
            seq,
            at,
            input_tokens: 0,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 0,
            runtime_ms,
        }
    }

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

#[cfg(test)]
mod tests;
