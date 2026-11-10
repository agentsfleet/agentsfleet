//! What one provider request used, as the runner reports it, and the priced
//! line the daemon makes of it.
//!
//! The daemon prices each request on its own
//! (`docs/architecture/billing_and_provider_keys.md`): a context tier
//! prices the WHOLE request, so a run's cumulative totals cannot say which
//! requests crossed a tier, and a price can change at a date mid-run. The
//! runner reports one [`RequestUsage`] per provider request; the daemon turns
//! each into a [`UsageLine`] and adds its own runtime-only line, measured on
//! its own clock, for the time the run held its lease.
//!
//! Every line says what it bills, as its [`LineKind`], and is stored once per
//! `(work, kind, seq)`, so a retried renewal or report re-sends lines
//! harmlessly. Each kind numbers its own lines: a model line by the runner's
//! request number, a runtime line by when its slice began, a decision line by
//! its call. No kind's numbers can claim another's, and none is capped below
//! the `u32` the wire carries (M245).

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

/// What one usage line bills.
///
/// Stored in `billing.usage_lines.kind` by [`LineKind::as_str`]; the spellings
/// live here, not in a schema `CHECK` (RULE STS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// A model call the runner made and reported, numbered by its request.
    Model,
    /// A slice of run time the daemon measured, numbered by the milliseconds
    /// from its work's start to the slice's start.
    Runtime,
    /// A native decision call the daemon made, numbered by its call.
    Decision,
}

impl LineKind {
    /// Every kind, for a reader that must name them all.
    pub const ALL: [Self; 3] = [Self::Model, Self::Runtime, Self::Decision];

    /// The spelling `billing.usage_lines.kind` stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Runtime => "runtime",
            Self::Decision => "decision",
        }
    }

    /// The kind a stored spelling names, if any.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == stored)
    }
}

/// Token counts for one provider request, without double counting.
///
/// Cached reads and writes are excluded from input. Reasoning counts once within output.
/// Cache writes with known lifetimes use their respective fields; unknown lifetimes use
/// `cache_write_tokens`. Runtime is measured separately by the server.
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
    #[garde(skip)]
    pub seq: u32,
    /// Request time as an RFC 3339 timestamp. The server limits it to the assignment's time
    /// window before calculating cost.
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
            kind: LineKind::Model,
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

/// Usage records for this renewal or final report, up to 512. An empty list still records
/// server-measured runtime.
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
    /// What the line bills.
    #[garde(skip)]
    pub kind: LineKind,
    /// This line's number within its work and kind.
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
    /// A runtime-only line: `runtime_ms` of run time measured at `at`, for the
    /// slice that began `seq` milliseconds after its work began.
    #[must_use]
    pub const fn runtime(seq: u32, at: Timestamp, runtime_ms: u32) -> Self {
        Self {
            kind: LineKind::Runtime,
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
    pub fn whole_prompt(&self) -> u64 {
        u64::from(self.input_tokens)
            + u64::from(self.cached_input_tokens)
            + u64::from(self.cache_write_tokens)
            + u64::from(self.cache_write_5m_tokens)
            + u64::from(self.cache_write_1h_tokens)
    }
}

#[cfg(test)]
mod tests;

/// Provider-reported observations; absent usage is not a measured zero.
/// This internal value is not serialized into a renewal or report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObservedTokens {
    /// Whole prompt, cache reads and writes included.
    pub input: Option<u64>,
    /// Completion, reasoning included once.
    pub output: Option<u64>,
    /// Cache-read subset of input.
    pub cache_read: Option<u64>,
    /// Cache-write subset of input, across lifetimes.
    pub cache_write: Option<u64>,
}
