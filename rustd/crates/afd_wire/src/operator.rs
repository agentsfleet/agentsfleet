//! Response shapes for the platform operator's runner views.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::admin::AdminState;
use crate::runner::{AssignedPolicy, CapabilityReport, RunnerLiveness, SelftestReport};

/// One runner in the newest-first operator list.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerItem<'a> {
    /// Canonical runner identifier.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Stable host identity supplied at enrolment.
    #[serde(borrow)]
    pub host_id: Cow<'a, str>,
    /// Assigned isolation tier spelling.
    #[serde(borrow)]
    pub sandbox_tier: Cow<'a, str>,
    /// Operator-controlled admission state.
    pub admin_state: AdminState,
    /// Runtime state derived from heartbeat and leases.
    pub liveness: RunnerLiveness,
    /// Placement labels.
    #[serde(borrow)]
    pub labels: Vec<Cow<'a, str>>,
    /// Last heartbeat instant in epoch milliseconds.
    pub last_seen_at: i64,
    /// Enrolment instant in epoch milliseconds.
    pub created_at: i64,
    /// Policy currently assigned to the host.
    #[serde(borrow)]
    pub assigned_policy: Option<AssignedPolicy<'a>>,
    /// Capability report most recently supplied by the host.
    #[serde(borrow)]
    pub achievable: Option<CapabilityReport<'a>>,
    /// Whether the assignment exceeds the reported capability.
    pub degraded: bool,
    /// Stored explanation for a degraded verdict.
    #[serde(borrow)]
    pub degraded_reason: Option<Cow<'a, str>>,
}

/// A keyset page of runner list rows.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnersResponse<'a> {
    /// Rows in newest-first order.
    #[serde(borrow)]
    pub items: Vec<RunnerItem<'a>>,
    /// Total runner rows independent of the boundary.
    pub total: i64,
    /// Cursor to send as `starting_after` on the next request.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

/// One runner with its live and lifetime counters.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerDetail<'a> {
    /// The fields shared with the runner list, flattened on the wire.
    #[serde(flatten, borrow)]
    pub item: RunnerItem<'a>,
    /// Live leases currently held.
    pub active_lease_count: i64,
    /// Fleets represented by live leases.
    pub active_fleet_count: i64,
    /// Lifetime acquired count.
    pub leases_acquired: i64,
    /// Lifetime successful count.
    pub leases_succeeded: i64,
    /// Lifetime failed count.
    pub leases_failed: i64,
    /// Lifetime expired count.
    pub leases_expired: i64,
    /// Outstanding self-test request instant.
    pub selftest_requested_at: Option<i64>,
    /// Latest self-test completion instant.
    pub selftest_completed_at: Option<i64>,
    /// Latest complete self-test report.
    #[serde(borrow)]
    pub selftest: Option<SelftestReport<'a>>,
}

/// Server-derived outcome of one runner lease.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseOutcome {
    /// The lease remains held by this runner.
    Running,
    /// Its Fleet event settled successfully.
    Succeeded,
    /// Its Fleet event settled as a fleet error.
    Failed,
    /// This holder was superseded by a reclaim.
    Expired,
    /// Stored state cannot establish a terminal outcome.
    Unknown,
}

/// Whether the lease was the event's first claim or a later reclaim.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseKind {
    /// First claim of the Fleet event.
    Fresh,
    /// Claim after an earlier holder expired.
    Reclaim,
}

/// One lease in a runner's newest-first operator history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerLeaseItem<'a> {
    /// Lease identifier and external pagination cursor.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Fleet whose event was leased.
    #[serde(borrow)]
    pub fleet_id: Cow<'a, str>,
    /// Current display name, absent if the fleet row is gone.
    #[serde(borrow)]
    pub fleet_name: Option<Cow<'a, str>>,
    /// Workspace owning the Fleet event.
    #[serde(borrow)]
    pub workspace_id: Cow<'a, str>,
    /// Fleet event identifier.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// Fleet event type.
    #[serde(borrow)]
    pub event_type: Cow<'a, str>,
    /// Actor recorded on the Fleet event.
    #[serde(borrow)]
    pub actor: Cow<'a, str>,
    /// Server-derived closed outcome.
    pub outcome: LeaseOutcome,
    /// Terminal failure classification, when one was stored.
    #[serde(borrow)]
    pub failure_label: Option<Cow<'a, str>>,
    /// Terminal failure explanation, when one was stored.
    #[serde(borrow)]
    pub failure_detail: Option<Cow<'a, str>>,
    /// Whether this was the first claim or a reclaim.
    pub kind: LeaseKind,
    /// Monotonic holder generation.
    pub fencing_token: i64,
    /// The provider the run used.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// The provider's own name for the model the run used.
    #[serde(borrow)]
    pub model: Cow<'a, str>,
    /// The credential posture the run used.
    #[serde(borrow)]
    pub posture: Cow<'a, str>,
    /// Metered non-cached input tokens.
    pub metered_input_tokens: i64,
    /// Metered cached input tokens.
    pub metered_cached_tokens: i64,
    /// Metered output tokens.
    pub metered_output_tokens: i64,
    /// Settled wall time, absent before settlement.
    pub wall_ms: Option<i64>,
    /// Holder deadline in epoch milliseconds.
    pub lease_expires_at: i64,
    /// Claim instant in epoch milliseconds.
    pub created_at: i64,
}

/// A keyset page of one runner's lease history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerLeasesResponse<'a> {
    /// Rows in newest-first order.
    #[serde(borrow)]
    pub items: Vec<RunnerLeaseItem<'a>>,
    /// Filtered total independent of the page boundary.
    pub total: i64,
    /// Last lease identifier when another page may exist.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "the canonical wire fixture must serialize during its test"
)]
mod tests {
    use super::*;

    #[test]
    fn runner_lease_page_has_the_exact_operator_wire_shape() {
        let page = RunnerLeasesResponse {
            items: vec![RunnerLeaseItem {
                id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010"),
                fleet_id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011"),
                fleet_name: Some(Cow::Borrowed("Production")),
                workspace_id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb012"),
                event_id: Cow::Borrowed("1725000000000-0"),
                event_type: Cow::Borrowed("chat"),
                actor: Cow::Borrowed("user:operator"),
                outcome: LeaseOutcome::Failed,
                failure_label: Some(Cow::Borrowed("provider_error")),
                failure_detail: Some(Cow::Borrowed("upstream refused")),
                kind: LeaseKind::Reclaim,
                fencing_token: 2,
                provider: Cow::Borrowed("anthropic"),
                model: Cow::Borrowed("claude-opus-4-1"),
                posture: Cow::Borrowed("platform"),
                metered_input_tokens: 101,
                metered_cached_tokens: 17,
                metered_output_tokens: 29,
                wall_ms: Some(1_250),
                lease_expires_at: 1_725_000_030_000,
                created_at: 1_725_000_000_000,
            }],
            total: 3,
            next_cursor: Some(Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010")),
        };

        assert_eq!(
            serde_json::to_value(page).expect("operator page serializes"),
            serde_json::json!({
                "items": [{
                    "id": "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010",
                    "fleet_id": "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011",
                    "fleet_name": "Production",
                    "workspace_id": "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb012",
                    "event_id": "1725000000000-0",
                    "event_type": "chat",
                    "actor": "user:operator",
                    "outcome": "failed",
                    "failure_label": "provider_error",
                    "failure_detail": "upstream refused",
                    "kind": "reclaim",
                    "fencing_token": 2,
                    "provider": "anthropic",
                    "model": "claude-opus-4-1",
                    "posture": "platform",
                    "metered_input_tokens": 101,
                    "metered_cached_tokens": 17,
                    "metered_output_tokens": 29,
                    "wall_ms": 1_250,
                    "lease_expires_at": 1_725_000_030_000_i64,
                    "created_at": 1_725_000_000_000_i64
                }],
                "total": 3,
                "next_cursor": "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010"
            })
        );
    }
}
