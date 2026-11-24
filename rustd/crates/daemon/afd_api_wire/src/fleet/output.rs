//! A fleet as one read answers it, and what its create answers.

use serde::Serialize;
use serde_json::Value;

/// Shared fleet resource, returned by create, read and every change.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct FleetResponse<'a, S = Value> {
    /// Fleet identity.
    pub id: String,
    /// Validated instance slug.
    pub name: String,
    /// `active` or `archived`.
    pub status: String,
    /// Strong resource tag for every change.
    pub etag: String,
    /// The workspace model the fleet's runs use; copy one from
    /// `GET /v1/workspaces/{workspace_id}/models`.
    pub model_ref: String,
    /// Where the fleet's HTTP tools may reach: `restricted` to the hosts of its
    /// approved access, or `public`. A new fleet is `restricted`.
    pub http_egress: af_wire::policy::HttpEgress,
    /// Whether its HTTP tools may also reach the package registries. A new
    /// fleet may not.
    pub package_domains: bool,
    /// Stored schedule observations.
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
    pub schedules: Vec<S>,
    /// What else may wake it, as its rows hold them.
    pub triggers: Vec<super::Trigger<'a>>,
    /// The secrets and integrations it asked for, and where each stands.
    pub grants: Vec<crate::grant::Grant<'a>>,
    /// Current independent shared-memory permissions.
    pub memory_access: super::MemoryAccess,
    /// Creation timestamp, such as `2026-10-24T05:00:00.000Z`.
    #[serde(with = "af_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Last update timestamp, such as `2026-10-24T05:00:00.000Z`.
    #[serde(with = "af_wire::instant")]
    pub updated_at: jiff::Timestamp,
    /// Exact lifetime event count.
    pub events_processed: String,
    /// Exact lifetime spend in nanos.
    #[cfg_attr(feature = "openapi", schema(value_type = af_wire::nanos::Decimal))]
    pub budget_used_nanos: String,
    /// Current unanswered approval gates.
    pub pending_approvals: i64,
}

/// Where a new fleet's first message went. Nothing runs ahead of it, so it
/// never joins a live run.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FirstTurnDisposition {
    /// A runner took it at once.
    Started,
    /// It waits for a runner.
    Queued,
}

/// The first message a create accepted, as the fleet's first turn.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirstTurn {
    /// The turn the message became; filter the fleet's turn stream on it.
    pub turn_id: String,
    /// Where it went.
    pub disposition: FirstTurnDisposition,
}

/// `POST /v1/workspaces/{workspace_id}/fleets`: the fleet and its first turn.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct CreatedFleet<'a, S = Value> {
    /// The created fleet.
    pub fleet: FleetResponse<'a, S>,
    /// The first message, as the fleet's first turn.
    pub accepted: FirstTurn,
    /// Whether an earlier create with the same `operation_id` already made
    /// this fleet: `true` on a repeat, which answers 200.
    pub replayed: bool,
}
