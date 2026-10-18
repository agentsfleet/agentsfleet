//! A model's evidence and dated prices, as both API planes spell them.
//!
//! Every rate is integer nanos (10⁻⁹ US dollars) per million tokens. Amounts
//! are exact integers on the wire, never decimals, so a client converts what an
//! operator types to nanos once and never round-trips a stored amount through
//! a float. A cache-write rate that is `null` is unknown or unsupported, never
//! free.

use std::borrow::Cow;

use afd_validate::spans::{Span, disjoint};
use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::admin_catalogue::{CONTEXT_CAP_MIN, RATE_NANOS_MIN};

/// The longest label a model shows.
pub const DISPLAY_NAME_MAX_BYTES: usize = 128;
/// The longest evidence URL a model or price records.
pub const EVIDENCE_URL_MAX_BYTES: usize = 2048;
/// The longest pricing-conditions note.
pub const CONDITIONS_MAX_BYTES: usize = 1024;
/// The most prices one model carries: every tier across every date range.
pub const PRICES_MAX: usize = 64;
/// The base tier's threshold: it applies from the first prompt token.
pub const BASE_TIER: i32 = 0;

/// What a schedule without a base tier reports.
pub const NO_BASE_TIER: &str = "prices must include a tier with prompt_tokens_above = 0";

/// One exact provider/model identity's label and context evidence.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CatalogueMetadata<'a> {
    /// Human-readable label; never used to route a request.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = DISPLAY_NAME_MAX_BYTES))]
    pub display_name: Cow<'a, str>,
    /// The exact served context limit, from the provider's own models API.
    #[garde(range(min = CONTEXT_CAP_MIN))]
    pub context_cap_tokens: i32,
    /// The provider models API response that supplied the limit.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = EVIDENCE_URL_MAX_BYTES))]
    pub context_source_url: Cow<'a, str>,
    /// When that response was verified.
    #[serde(with = "afd_wire::instant")]
    #[garde(skip)]
    pub context_verified_at: jiff::Timestamp,
    /// Whether a picker lists the model only when asked for advanced models.
    #[garde(skip)]
    pub advanced: bool,
}

/// One prompt-size tier's rates during a half-open date range.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CataloguePrice<'a> {
    /// The first instant this quote applies.
    #[serde(with = "afd_wire::instant")]
    #[garde(skip)]
    pub effective_from: jiff::Timestamp,
    /// The first instant it no longer applies, or `null` for an open range.
    #[serde(with = "afd_wire::instant::option", default)]
    #[garde(skip)]
    pub effective_until: Option<jiff::Timestamp>,
    /// The tier applies to a request whose whole prompt exceeds this many
    /// tokens; `0` is the base tier.
    #[garde(range(min = BASE_TIER))]
    pub prompt_tokens_above: i32,
    /// Uncached input.
    #[garde(range(min = RATE_NANOS_MIN))]
    pub input_nanos_per_mtok: i64,
    /// Input read from the provider's cache.
    #[garde(range(min = RATE_NANOS_MIN))]
    pub cached_input_nanos_per_mtok: i64,
    /// Cache writes whose provider reports no lifetime.
    #[serde(default)]
    #[garde(inner(range(min = RATE_NANOS_MIN)))]
    pub cache_write_nanos_per_mtok: Option<i64>,
    /// Cache writes with a five-minute lifetime.
    #[serde(default)]
    #[garde(inner(range(min = RATE_NANOS_MIN)))]
    pub cache_write_5m_nanos_per_mtok: Option<i64>,
    /// Cache writes with a one-hour lifetime.
    #[serde(default)]
    #[garde(inner(range(min = RATE_NANOS_MIN)))]
    pub cache_write_1h_nanos_per_mtok: Option<i64>,
    /// Output, reasoning included once where the provider bills it.
    #[garde(range(min = RATE_NANOS_MIN))]
    pub output_nanos_per_mtok: i64,
    /// The provider's pricing page or response the quote came from.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = EVIDENCE_URL_MAX_BYTES))]
    pub source_url: Cow<'a, str>,
    /// When the quote was verified.
    #[serde(with = "afd_wire::instant")]
    #[garde(skip)]
    pub verified_at: jiff::Timestamp,
    /// Scope, exclusions and provider-specific conditions; may be empty.
    #[serde(borrow)]
    #[garde(length(bytes, max = CONDITIONS_MAX_BYTES))]
    pub conditions: Cow<'a, str>,
}

/// A complete price schedule: a base tier, and no two quotes for one tier
/// applying at the same instant.
///
/// # Errors
/// [`NO_BASE_TIER`], or the span rule's refusal for a reversed or overlapping
/// date range.
pub fn price_schedule<C: ?Sized>(prices: &[CataloguePrice<'_>], _context: &C) -> garde::Result {
    if !prices
        .iter()
        .any(|price| price.prompt_tokens_above == BASE_TIER)
    {
        return Err(garde::Error::new(NO_BASE_TIER));
    }
    disjoint(prices.iter().map(|price| Span {
        key: price.prompt_tokens_above,
        start: price.effective_from,
        end: price.effective_until,
    }))
}

#[cfg(test)]
mod tests;
