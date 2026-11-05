//! Native plans, renewable pins and manifest publication.

use garde::Validate;
use serde::{Deserialize, Serialize};
use crate::task::DecisionTaskVersion;

/// A verified base is retained separately from task recovery pointers.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointMode { RetainBase, Incremental }

impl CheckpointMode {
    /// Canonical native category.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self { Self::RetainBase => "retain_base", Self::Incremental => "incremental" }
    }
}

/// Allocate immutable page identities before capture uploads.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPlanInput {
    /// Existing fleet-slot authority.
    #[garde(skip)] pub fencing_token: u64,
    /// Retry identity; replay returns this exact plan.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))] pub operation_id: String,
    /// Current objective version.
    #[garde(dive)] pub task: DecisionTaskVersion,
    /// Initial retained source or an incremental logical state.
    #[garde(skip)] pub mode: CheckpointMode,
    /// An already verified base; omitted for repository-free tasks.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")))] pub base_id: Option<String>,
    /// Captured repository head; an incremental plan can use an older retained base.
    #[garde(inner(length(bytes, min = 40, max = 40), pattern(r"^[0-9a-f]{40}$")))] pub commit_oid: Option<String>,
    /// Restore format, independently checked by the daemon.
    #[garde(range(min = 1, max = 1))] pub format_version: u32,
    /// Pinned host-local toolbox, excluded from checkpoint bytes.
    #[garde(custom(super::digest))] pub toolbox_digest: String,
    /// Ordered page identities to allocate.
    #[garde(range(min = 1, max = 64))] pub page_count: u32,
    /// Declared unique new content allowance, clamped natively by mode.
    #[garde(range(max = 17_179_869_184))] pub upload_budget_bytes: u64,
    /// Expanded logical state, independent of deduplication.
    #[garde(range(max = 25_769_803_776))] pub logical_bytes: u64,
    /// Expanded entries, checked again against complete pages.
    #[garde(range(max = 200_000))] pub entry_count: u32,
}

/// One capture or restore operation, renewed without extending its deadline.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointPin {
    /// Native pin identity.
    pub id: String,
    /// Compare-and-swap version for renewal.
    pub resource_version: u64,
    /// Hard operation deadline.
    #[serde(with = "crate::instant")] pub operation_deadline: jiff::Timestamp,
    /// Current liveness deadline.
    #[serde(with = "crate::instant")] pub expires_at: jiff::Timestamp,
}

/// Native recovery descriptor, bounded independently of manifest size.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointPlan {
    /// Native task-bound plan.
    pub checkpoint_id: String,
    /// Checkpoint ordering, distinct from the task generation.
    pub generation: u64,
    /// Retained base identity when present.
    pub base_id: Option<String>,
    /// Base or incremental state.
    pub mode: CheckpointMode,
    /// Repository identity selected by native task authority.
    pub repository: String,
    /// Required restore format.
    pub format_version: u32,
    /// Host-local toolbox compatibility.
    pub toolbox_digest: String,
    /// Complete ordered pages; never an unbounded inline manifest.
    pub page_ids: Vec<String>,
    /// Verified retained base pages, read through the same scoped page resource.
    pub base_page_ids: Vec<String>,
    /// Ordered retained-base root, checked before overlaying local changes.
    pub base_manifest_root: Option<String>,
    /// Exact base commit, independent of a later published branch head.
    pub base_commit_oid: Option<String>,
    /// Absent until native verification commits.
    pub manifest_root: Option<String>,
    /// History state bound to the same file generation.
    pub history_digest: Option<String>,
    /// Inputs actually incorporated in this state.
    pub covered_input_sequence: u64,
    /// Confirmed outcomes incorporated in this state.
    pub effect_watermark: u64,
    /// Deadline established once when the plan is created.
    #[serde(with = "crate::instant")] pub expires_at: jiff::Timestamp,
    /// Current writer/reader pin.
    pub pin: CheckpointPin,
    /// Immutable plan replay.
    pub replayed: bool,
}

/// Versioned reader/writer renewal.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPinInput {
    /// Existing lease fence.
    #[garde(skip)] pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)] pub task: DecisionTaskVersion,
    /// Existing pin version.
    #[garde(range(min = 1))] pub resource_version: u64,
}

/// Commit only after complete pages and registered objects verify.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointCommitInput {
    /// Existing lease fence.
    #[garde(skip)] pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)] pub task: DecisionTaskVersion,
    /// Current writer pin.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))] pub pin_id: String,
    /// Canonical ordered page-root digest.
    #[garde(custom(super::digest))] pub manifest_root: String,
    /// Complete compatible conversation digest.
    #[garde(custom(super::digest))] pub history_digest: String,
    /// Inputs included in the saved state.
    #[garde(skip)] pub covered_input_sequence: u64,
    /// Confirmed effects incorporated locally, never merely observed remotely.
    #[garde(skip)] pub effect_watermark: u64,
}

/// Supervisor-only read purpose.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointTransferPurpose { Upload, Restore }

/// Current authority for bounded manifest/object reads.
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointReadQuery {
    /// Current lease fence.
    #[garde(skip)] pub fencing_token: u64,
    /// Current restore pin.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))] pub pin_id: String,
    /// Ordered object cursor.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")))] pub starting_after: Option<String>,
}
