//! The workspace's fleet directory.

use serde::Serialize;
use std::borrow::Cow;

/// A shared fleet in the workspace directory.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct FleetSummary<'a> {
    /// Fleet identity.
    pub id: Cow<'a, str>,
    /// Validated instance slug.
    pub name: Cow<'a, str>,
    /// Current lifecycle state.
    pub status: Cow<'a, str>,
    /// Creation time.
    #[serde(with = "af_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Last mutation.
    #[serde(with = "af_wire::instant")]
    pub updated_at: jiff::Timestamp,
    /// Exact lifetime event count.
    pub events_processed: String,
    /// Exact lifetime spend in nanos.
    #[cfg_attr(feature = "openapi", schema(value_type = af_wire::nanos::Decimal))]
    pub budget_used_nanos: String,
}

/// One directory page, newest first.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct FleetsResponse<'a> {
    /// Rows on this page.
    pub items: Vec<FleetSummary<'a>>,
    /// Unknown without a separate count; always null.
    pub total: Option<usize>,
    /// Use this value unchanged for the next page, or stop when null.
    pub next_cursor: Option<Cow<'a, str>>,
}
