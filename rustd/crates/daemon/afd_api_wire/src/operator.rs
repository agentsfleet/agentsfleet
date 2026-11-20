//! Response shapes for the platform operator's runner views.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::admin::AdminState;
use af_wire::runner::{AssignedPolicy, CapabilityReport, RunnerLiveness, SelftestReport};

/// Why the daemon has stopped offering new work to an operator-active host.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CordonReason {
    /// No heartbeat reached the daemon within its offline threshold.
    MissedHeartbeats,
}

impl CordonReason {
    /// Stable database, audit and API spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissedHeartbeats => "missed_heartbeats",
        }
    }
}

/// Automatic quarantine; operator status remains independently visible.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerCordon {
    /// Trigger that blocks new work.
    pub reason: CordonReason,
    /// Committed quarantine instant.
    #[serde(with = "af_wire::instant")]
    pub cordoned_at: jiff::Timestamp,
}

/// One runner in the newest-first operator list.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerItem<'a> {
    /// Automatic quarantine; absent when the daemon permits new work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cordon: Option<RunnerCordon>,
    /// Canonical runner identifier.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// The tenant that registered this runner. Absent for a platform runner.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<Cow<'a, str>>,
    /// Stable host identity supplied at enrolment.
    #[serde(borrow)]
    pub host_id: Cow<'a, str>,
    /// Assigned isolation tier spelling.
    #[serde(borrow)]
    pub sandbox_tier: Cow<'a, str>,
    /// Operator-controlled admission state.
    pub status: AdminState,
    /// Runtime state derived from heartbeat and runs.
    pub liveness: RunnerLiveness,
    /// Placement labels.
    #[serde(borrow)]
    pub labels: Vec<Cow<'a, str>>,
    /// Last heartbeat instant.
    #[serde(with = "af_wire::instant")]
    pub last_seen_at: jiff::Timestamp,
    /// Enrolment instant.
    #[serde(with = "af_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Policy currently assigned to the host.
    #[serde(borrow)]
    pub assigned_policy: AssignedPolicy<'a>,
    /// Capability report most recently supplied by the host.
    #[serde(borrow)]
    pub achievable: Option<CapabilityReport<'a>>,
    /// Whether the assignment exceeds the reported capability.
    pub degraded: bool,
    /// Stored explanation for a degraded verdict.
    #[serde(borrow)]
    pub degraded_reason: Option<Cow<'a, str>>,
}

/// One page of runners.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerDetail<'a> {
    /// The fields shared with the runner list, flattened on the wire.
    #[serde(flatten, borrow)]
    pub item: RunnerItem<'a>,
    /// Live runs currently held.
    pub active_run_count: i64,
    /// Fleets represented by live runs.
    pub active_fleet_count: i64,
    /// Lifetime acquired count.
    pub runs_acquired: i64,
    /// Lifetime successful count.
    pub runs_succeeded: i64,
    /// Lifetime failed count.
    pub runs_failed: i64,
    /// Lifetime expired count.
    pub runs_expired: i64,
    /// Outstanding self-test request instant.
    #[serde(with = "af_wire::instant::option", default)]
    pub selftest_requested_at: Option<jiff::Timestamp>,
    /// Latest self-test completion instant.
    #[serde(with = "af_wire::instant::option", default)]
    pub selftest_completed_at: Option<jiff::Timestamp>,
    /// Latest complete self-test report.
    #[serde(borrow)]
    pub selftest: Option<SelftestReport<'a>>,
}

/// Server-derived outcome of one runner run.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// The run remains held by this runner.
    Running,
    /// Its fleet turn settled successfully.
    Succeeded,
    /// Its fleet turn settled as a fleet error.
    Failed,
    /// This holder was superseded by a reclaim.
    Expired,
    /// A person stopped its turn, and the run reported stopped.
    Stopped,
    /// Stored state cannot establish a terminal outcome.
    Unknown,
}

/// Whether the signed-in operator can open the run's fleet.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunDestination {
    /// The fleet exists in the caller's own account.
    Available,
    /// The fleet was purged; the run is retained runner history.
    FleetDeleted,
    /// The fleet belongs to an account the caller is not acting in.
    NotPermitted,
}

/// Whether the run was the turn's first claim or a later reclaim.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    /// First claim of the fleet turn.
    Fresh,
    /// Claim after an earlier holder expired.
    Reclaim,
}

/// One run in a runner's newest-first operator history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerRunItem<'a> {
    /// Run identifier and external pagination cursor.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Fleet whose turn was held.
    #[serde(borrow)]
    pub fleet_id: Cow<'a, str>,
    /// Current display name, absent if the fleet row is gone.
    #[serde(borrow)]
    pub fleet_name: Option<Cow<'a, str>>,
    /// Whether the caller can open the fleet. Never inferred from the name.
    pub destination: RunDestination,
    /// Workspace owning the fleet turn.
    #[serde(borrow)]
    pub workspace_id: Cow<'a, str>,
    /// Fleet turn identifier.
    #[serde(borrow)]
    pub turn_id: Cow<'a, str>,
    /// Fleet turn type, absent once the turn row is gone.
    #[serde(borrow)]
    pub event_type: Option<Cow<'a, str>>,
    /// Actor recorded on the fleet turn, absent once the turn row is gone.
    #[serde(borrow)]
    pub actor: Option<Cow<'a, str>>,
    /// Server-derived closed outcome.
    pub outcome: RunOutcome,
    /// Terminal failure classification, when one was stored.
    #[serde(borrow)]
    pub failure_label: Option<Cow<'a, str>>,
    /// Terminal failure explanation, when one was stored.
    #[serde(borrow)]
    pub failure_detail: Option<Cow<'a, str>>,
    /// Whether this was the first claim or a reclaim.
    pub kind: RunKind,
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
    /// Holder deadline.
    #[serde(with = "af_wire::instant")]
    pub run_expires_at: jiff::Timestamp,
    /// Claim instant.
    #[serde(with = "af_wire::instant")]
    pub created_at: jiff::Timestamp,
}

/// One page of a runner's work-assignment history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerRunsResponse<'a> {
    /// Rows in newest-first order.
    #[serde(borrow)]
    pub items: Vec<RunnerRunItem<'a>>,
    /// Filtered total independent of the page boundary.
    pub total: i64,
    /// Last run identifier when another page may exist.
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
    fn runner_run_page_has_the_exact_operator_wire_shape() {
        let page = RunnerRunsResponse {
            items: vec![RunnerRunItem {
                id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010"),
                fleet_id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011"),
                fleet_name: Some(Cow::Borrowed("Production")),
                destination: RunDestination::Available,
                workspace_id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb012"),
                turn_id: Cow::Borrowed("1725000000000-0"),
                event_type: Some(Cow::Borrowed("chat")),
                actor: Some(Cow::Borrowed("user:operator")),
                outcome: RunOutcome::Failed,
                failure_label: Some(Cow::Borrowed("provider_error")),
                failure_detail: Some(Cow::Borrowed("upstream refused")),
                kind: RunKind::Reclaim,
                fencing_token: 2,
                provider: Cow::Borrowed("anthropic"),
                model: Cow::Borrowed("claude-opus-4-1"),
                posture: Cow::Borrowed("platform"),
                metered_input_tokens: 101,
                metered_cached_tokens: 17,
                metered_output_tokens: 29,
                wall_ms: Some(1_250),
                run_expires_at: jiff::Timestamp::constant(1_725_000_030, 0),
                created_at: jiff::Timestamp::constant(1_725_000_000, 0),
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
                    "destination": "available",
                    "workspace_id": "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb012",
                    "turn_id": "1725000000000-0",
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
                    "run_expires_at":"2024-08-30T06:40:30Z",
                    "created_at":"2024-08-30T06:40:00Z"
                }],
                "total": 3,
                "next_cursor": "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010"
            })
        );
    }
}
