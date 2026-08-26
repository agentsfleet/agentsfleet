//! The operator plane for a runner: its state vocabulary and history.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::runner::AssignedPolicy;

/// `PUT /v1/admin/platform-keys` metadata; key bytes already live in the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeyPut<'a> {
    /// Provider and vault-row name.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Workspace holding that vault row.
    #[serde(borrow)]
    pub source_workspace_id: Cow<'a, str>,
    /// Priced model selected as platform default.
    #[serde(borrow)]
    pub model: Cow<'a, str>,
    /// Custom endpoint for the compatible-provider mode.
    #[serde(borrow)]
    pub base_url: Option<Cow<'a, str>>,
}

/// Reveal-free platform-key list item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeyItem<'a> {
    /// Provider and vault-row name.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Workspace holding the key.
    #[serde(borrow)]
    pub source_workspace_id: Cow<'a, str>,
    /// Active priced model, absent after deactivation.
    #[serde(borrow)]
    pub model: Option<Cow<'a, str>>,
    /// Whether this row is the platform default.
    pub active: bool,
    /// Last mutation instant in epoch milliseconds.
    pub updated_at: i64,
}

/// `GET /v1/admin/platform-keys` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeysResponse<'a> {
    /// Every active and inactive provider row.
    #[serde(borrow)]
    pub keys: Vec<PlatformKeyItem<'a>>,
}

/// Mutable rate fields shared by admin model create and patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRates {
    /// Maximum context tokens.
    pub context_cap_tokens: i32,
    /// Input-token nanos per million tokens.
    pub input_nanos_per_mtok: i64,
    /// Cached-input nanos per million tokens.
    pub cached_input_nanos_per_mtok: i64,
    /// Output-token nanos per million tokens.
    pub output_nanos_per_mtok: i64,
}

/// `POST /v1/admin/models` input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelCreate<'a> {
    /// Provider identity.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Provider-native model identity.
    #[serde(borrow)]
    pub model_id: Cow<'a, str>,
    /// Rates and context cap flattened on the existing wire.
    #[serde(flatten)]
    pub rates: ModelRates,
}

/// One priced model row in the admin list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelItem<'a> {
    /// Opaque `UUIDv7` row identity.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Provider identity.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Provider-native model identity.
    #[serde(borrow)]
    pub model_id: Cow<'a, str>,
    /// Rates and context cap flattened on the existing wire.
    #[serde(flatten)]
    pub rates: ModelRates,
}

/// `GET /v1/admin/models` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelsResponse<'a> {
    /// Every priced row.
    #[serde(borrow)]
    pub models: Vec<AdminModelItem<'a>>,
}

/// One metadata-only platform Fleet-library row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryItem<'a> {
    /// Slug identity.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Curated description.
    #[serde(borrow)]
    pub description: Cow<'a, str>,
    /// GitHub owner/repository.
    #[serde(borrow)]
    pub source_repo: Cow<'a, str>,
    /// Fetched revision.
    #[serde(borrow)]
    pub source_ref: Cow<'a, str>,
    /// Draft or public.
    #[serde(borrow)]
    pub visibility: Cow<'a, str>,
    /// Content identity, never the support-file bytes.
    #[serde(borrow)]
    pub content_hash: Option<Cow<'a, str>>,
    /// Credential names only.
    pub required_credentials: serde_json::Value,
    /// Required tool names.
    pub required_tools: serde_json::Value,
    /// Declared outbound hosts.
    pub network_hosts: serde_json::Value,
    /// Whether a trigger document exists.
    pub trigger_present: bool,
    /// Last mutation instant in epoch milliseconds.
    pub updated_at: i64,
}

/// Admin Fleet-library list response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibrariesResponse<'a> {
    /// Every draft and public row.
    #[serde(borrow)]
    pub libraries: Vec<AdminLibraryItem<'a>>,
}

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

/// Platform-admin mutation actions.
///
/// `SelfTest` is the odd one out: the others transition state, while it records
/// an operator's ASK and leaves the state alone. It rides the same endpoint
/// because it shares everything that matters at the boundary — the operator
/// scope, the exactly-one-of body guard, and the refusal on a revoked runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerAdminAction {
    /// Stop giving it new work.
    Cordon,
    /// Let it finish, then stop.
    Drain,
    /// Revoke its credential.
    Revoke,
    /// Ask it to self-test on its next beat.
    SelfTest,
}

/// `PATCH /v1/fleets/runners/{id}` body.
///
/// Exactly one of `action` or `assigned_policy`; both or neither is a `400`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerAdminPatchRequest<'a> {
    /// A state transition or a self-test request.
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
