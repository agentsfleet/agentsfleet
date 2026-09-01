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

/// One priced model as the catalogue shows it.
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
    pub input_nanos_per_mtok: i64,
    /// The cached-input rate, likewise.
    pub cached_input_nanos_per_mtok: i64,
    /// The output rate, likewise.
    pub output_nanos_per_mtok: i64,
}

/// `GET /v1/models` — one page of the catalogue.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogueResponse<'a> {
    /// The catalogue's version stamp: the newest row's change date, UTC.
    pub version: Cow<'a, str>,
    /// The rows on this page, in the catalogue's normalized order.
    pub models: Vec<CatalogueModel<'a>>,
    /// Always `null` — never counted, always present.
    pub total: Option<i64>,
    /// Where the next page resumes, or `null` on the last page.
    pub next_cursor: Option<Cow<'a, str>>,
}
