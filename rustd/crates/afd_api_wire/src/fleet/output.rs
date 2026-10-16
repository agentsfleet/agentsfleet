//! Native setup projections are borrowed through serialization without a JSON copy.

use serde::Serialize;
use serde_json::Value;
use std::borrow::Cow;

/// Public manual-webhook address for one declared source.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebhookUrl<'a> {
    /// Declared provider.
    pub source: Cow<'a, str>,
    /// Deployment-derived address.
    pub url: Cow<'a, str>,
}

/// Shared fleet resource, returned by create, read and lifecycle changes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct FleetResponse<D = Value, P = Value, O = Value, S = Value> {
    /// Fleet identity.
    pub id: String,
    /// Validated instance slug.
    pub name: String,
    /// Current lifecycle state.
    pub status: String,
    /// Strong resource tag for name/status changes.
    pub etag: String,
    /// Strong setup tag for reviewed improvement application.
    pub setup_etag: String,
    /// Exact decimal immutable setup revision.
    pub setup_revision: String,
    /// Pinned policy interpretation.
    pub policy_semantics: String,
    /// Effective authority changed since review.
    pub needs_re_review: bool,
    /// Complete validated native setup, with no private transcript.
    #[cfg_attr(feature = "openapi", schema(value_type = afd_fleet_runtime::definition::DefinitionSchema))]
    pub definition: D,
    /// Live prerequisites and authorized recovery actions.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub setup: P,
    /// Explicit initial-run progress.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub start: O,
    /// Actual stored schedule observations.
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
    pub schedules: Vec<S>,
    /// Manual webhook addresses from the declared setup.
    pub webhook_urls: Vec<WebhookUrl<'static>>,
    /// Creation time in Unix milliseconds.
    pub created_at: i64,
    /// Last mutation in Unix milliseconds.
    pub updated_at: i64,
    /// Exact lifetime event count.
    pub events_processed: String,
    /// Exact lifetime spend in nanos.
    pub budget_used_nanos: String,
    /// Current unanswered approval gates.
    pub pending_approvals: i64,
    /// Current independent shared-memory permissions.
    pub memory_access: super::MemoryAccess,
}
