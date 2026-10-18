//! The model catalogue's payload: `GET /v1/models`.
//!
//! # `models`, not `items`
//!
//! The v1 response shipped with `models`, and renaming a shipped field is
//! what `docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids — `total` and
//! `next_cursor` were ADDED beside it when the page landed, so the envelope
//! is navigable without breaking a client. `total` is always `null`: counting
//! a keyset page costs the scan the pagination exists to avoid, and the key
//! stays present rather than omitted.

use std::borrow::Cow;

use serde::Serialize;

use crate::catalogue::CataloguePrice;

/// One priced model as the catalogue shows it. Every rate is integer nanos per
/// million tokens.
///
/// `id` is the MODEL identifier (`claude-sonnet-5`, `accounts/fireworks/…`),
/// never the row's own id — that one is admin-plane identity and rides the
/// cursor opaquely instead.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogueModel<'a> {
    /// The model identifier a tenant selects by.
    pub id: Cow<'a, str>,
    /// The provider hosting it.
    pub provider: Cow<'a, str>,
    /// The context window the price is quoted for.
    pub context_cap_tokens: i32,
    /// The input rate, in nanos per million tokens.
    #[serde(with = "afd_wire::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = afd_wire::nanos::Decimal))]
    pub input_nanos_per_mtok: i64,
    /// The cached-input rate, likewise.
    #[serde(with = "afd_wire::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = afd_wire::nanos::Decimal))]
    pub cached_input_nanos_per_mtok: i64,
    /// The output rate, likewise.
    #[serde(with = "afd_wire::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = afd_wire::nanos::Decimal))]
    pub output_nanos_per_mtok: i64,
    /// The label a picker shows; never used to route a request.
    pub display_name: Cow<'a, str>,
    /// Listed only when the caller asks for advanced models.
    pub advanced: bool,
    /// The provider models API response the context limit came from.
    pub context_source_url: Cow<'a, str>,
    /// When that response was verified.
    #[serde(with = "afd_wire::instant")]
    pub context_verified_at: jiff::Timestamp,
    /// The complete schedule, ordered by tier then start. The three rates
    /// above are its base tier as it stands now.
    pub prices: Vec<CataloguePrice<'a>>,
}

/// `GET /v1/models` — one page of the catalogue.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogueResponse<'a> {
    /// The catalogue's version stamp: the newest row's change date, UTC.
    /// Positive timestamps beyond the supported calendar range use their decimal
    /// Unix timestamp in milliseconds instead of a date.
    pub version: Cow<'a, str>,
    /// The rows on this page, in the catalogue's normalized order.
    pub models: Vec<CatalogueModel<'a>>,
    /// Always `null` — never counted, always present.
    pub total: Option<i64>,
    /// Where the next page resumes, or `null` on the last page.
    pub next_cursor: Option<Cow<'a, str>>,
}
