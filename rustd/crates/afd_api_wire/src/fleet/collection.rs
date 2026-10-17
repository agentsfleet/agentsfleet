//! Collection rows retain their lightweight trigger projection.

use serde::Serialize;
use serde_json::value::RawValue;
use std::borrow::Cow;

/// Validated stored trigger JSON, omitted when unavailable.
pub type Triggers = Option<Box<RawValue>>;

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
    /// Creation time in Unix milliseconds.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Last mutation in Unix milliseconds.
    #[serde(with = "afd_wire::instant")]
    pub updated_at: jiff::Timestamp,
    /// Authored trigger projection.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub triggers: Triggers,
    /// Exact lifetime event count.
    pub events_processed: String,
    /// Exact lifetime spend in nanos.
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
    /// Next opaque boundary, null on the final page.
    pub next_cursor: Option<Cow<'a, str>>,
}
