//! Response shapes for the platform operator's runner views.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::admin::AdminState;
use crate::runner::{AssignedPolicy, CapabilityReport, RunnerLiveness, SelftestReport};

/// One runner in the newest-first operator list.
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
