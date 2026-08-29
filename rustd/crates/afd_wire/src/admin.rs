//! The operator plane for a runner: its state vocabulary and history.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

pub use crate::admin_catalogue::{
    AdminLibrariesResponse, AdminLibraryCreated, AdminLibraryImport, AdminLibraryItem,
    AdminLibraryPatch, AdminLibraryRequirements, AdminModelCreate, AdminModelCreated,
    AdminModelItem, AdminModelUpdated, AdminModelsResponse, FleetBundleItem, FleetBundlesResponse,
    ModelRates, PlatformKeyDeactivateResponse, PlatformKeyItem, PlatformKeyPut,
    PlatformKeySetResponse, PlatformKeysResponse,
};
use crate::runner::AssignedPolicy;

/// Operator intent for a runner. Only `Active` admits a runner-plane call;
/// every other value rejects one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminState {
    /// Admitted; may lease and report.
    Active,
    /// Refused new work, existing work untouched.
    Cordoned,
    /// Finishing current work, taking no more.
    Draining,
    /// Finished draining.
    Drained,
    /// Credential revoked; every call rejected.
    Revoked,
}

/// Platform-operator mutation actions.
///
/// Rotation and self-test are operations rather than state transitions. They
/// ride the same endpoint because they share the operator scope and the
/// exactly-one-of body guard with the transition actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerAdminAction {
    /// Stop giving it new work.
    Cordon,
    /// Let it finish, then stop.
    Drain,
    /// Revoke its credential.
    Revoke,
    /// Replace its credential and reveal the replacement once.
    Rotate,
    /// Ask it to self-test on its next beat.
    SelfTest,
}

/// Successful runner-token rotation.
///
/// The replacement token is returned once. The daemon stores only its digest,
/// so no later read can recover this value.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerTokenRotatedResponse<'a> {
    /// The runner whose credential changed.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// The replacement bearer token. Secret — reveal once, then discard.
    #[serde(borrow)]
    pub runner_token: Cow<'a, str>,
}

/// `PATCH /v1/fleets/runners/{id}` body.
///
/// Exactly one of `action` or `assigned_policy`; both or neither is a `400`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerAdminPatchRequest<'a> {
    /// A state transition, token rotation, or self-test request.
    pub action: Option<RunnerAdminAction>,
    /// A policy re-assignment, which reaches the host on its next heartbeat.
    #[serde(borrow)]
    pub assigned_policy: Option<AssignedPolicy<'a>>,
}

/// `PATCH /v1/fleets/runners/{id}` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerAdminPatchResponse<'a> {
    /// The runner that changed.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Its state after the change.
    pub admin_state: AdminState,
    /// The assignment as stored, on the policy-update path.
    #[serde(borrow)]
    pub assigned_policy: Option<AssignedPolicy<'a>>,
    /// When the self-test request was recorded, epoch milliseconds.
    ///
    /// The reply is the REQUEST, never a verdict — the daemon picks the ask up
    /// on its next beat, so a result cannot exist yet. Returning the instant
    /// lets a reader age the pending state instead of spinning with no clock.
    pub selftest_requested_at: Option<i64>,
}

/// Append-only runner history values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerEventType {
    /// Enrolled.
    RunnerRegistered,
    /// Became reachable.
    RunnerOnline,
    /// Stopped being reachable.
    RunnerOffline,
    /// Took a lease.
    LeaseAcquired,
    /// Released a lease.
    LeaseReleased,
    /// Stopped taking new work.
    RunnerCordoned,
    /// Began finishing current work.
    RunnerDraining,
    /// Finished draining.
    RunnerDrained,
    /// Credential revoked.
    RunnerRevoked,
    /// Credential digest replaced without exposing the previous credential.
    RunnerTokenRotated,
    /// An operator re-assigned its policy — a security-posture change worth
    /// auditing.
    RunnerPolicyAssigned,
}

impl RunnerEventType {
    /// The stable spelling stored in Postgres and accepted by query filters.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunnerRegistered => "runner_registered",
            Self::RunnerOnline => "runner_online",
            Self::RunnerOffline => "runner_offline",
            Self::LeaseAcquired => "lease_acquired",
            Self::LeaseReleased => "lease_released",
            Self::RunnerCordoned => "runner_cordoned",
            Self::RunnerDraining => "runner_draining",
            Self::RunnerDrained => "runner_drained",
            Self::RunnerRevoked => "runner_revoked",
            Self::RunnerTokenRotated => "runner_token_rotated",
            Self::RunnerPolicyAssigned => "runner_policy_assigned",
        }
    }
}

/// The per-work tags: one acquired and one released per lease.
///
/// They dominate the table by construction and restate what the lease row
/// already carries, so retention prunes these and only these. One definition, so
/// a new per-work tag cannot be added without deciding which side it lands on.
pub const PER_LEASE_EVENT_TYPES: [RunnerEventType; 2] = [
    RunnerEventType::LeaseAcquired,
    RunnerEventType::LeaseReleased,
];

/// One row of runner history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerEventItem<'a> {
    /// The row's identifier.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// The runner it describes.
    #[serde(borrow)]
    pub runner_id: Cow<'a, str>,
    /// What happened.
    pub event_type: RunnerEventType,
    /// When, in epoch milliseconds.
    pub occurred_at: i64,
    /// Event-specific detail. Free-form by design.
    pub metadata: serde_json::Value,
}

/// A page of runner history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerEventsResponse<'a> {
    /// The rows in this page.
    #[serde(borrow)]
    pub items: Vec<RunnerEventItem<'a>>,
    /// Total rows matching the query.
    pub total: i64,
    /// Cursor for the next page, or null at the end.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

/// One live Server-Sent Events connection visible to platform operators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetStreamItem<'a> {
    /// Workspace whose events the connection may observe.
    #[serde(borrow)]
    pub workspace_id: Cow<'a, str>,
    /// Fleet whose events the connection may observe.
    #[serde(borrow)]
    pub fleet_id: Cow<'a, str>,
    /// Connection start instant in epoch milliseconds.
    pub started_ms: i64,
}

/// The instance-local live stream overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetStreamsResponse<'a> {
    /// Every live stream on this daemon instance.
    #[serde(borrow)]
    pub items: Vec<FleetStreamItem<'a>>,
    /// Current number of live streams.
    pub total: usize,
    /// Instance-wide admission ceiling.
    pub max_streams: u32,
}

#[cfg(test)]
#[path = "admin/tests.rs"]
mod tests;
