//! Immutable native publication requests and recoverable effect receipts.

use super::DecisionTaskVersion;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// Complete content for one supported regular-file change.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskFileChange {
    /// Canonical relative Git path; no Git metadata or workflow changes.
    #[garde(length(bytes, min = 1, max = 1024), custom(file_path))]
    pub path: String,
    /// Standard base64 contents; null deletes a regular file.
    #[garde(inner(length(bytes, max = 1_398_104)))]
    pub contents: Option<String>,
}

/// The supervisor has tested this exact tree against the recorded expected head.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskEffectInput {
    /// Stable identity; a lost acknowledgement must reuse this operation.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub operation_id: String,
    /// Current slot fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Current selected task and generation.
    #[garde(dive)]
    pub task: DecisionTaskVersion,
    /// Accepted originating action item.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub source_item_id: String,
    /// Every accepted current-task input through this cursor was considered.
    #[garde(skip)]
    pub covered_input_sequence: u64,
    /// Exact native old head; GitHub compares it atomically during publication.
    #[garde(length(bytes, min = 40, max = 40), pattern(r"^[0-9a-f]{40}$"))]
    pub expected_head: String,
    /// Git tree actually tested in the isolated workspace.
    #[garde(length(bytes, min = 40, max = 40), pattern(r"^[0-9a-f]{40}$"))]
    pub tested_tree: String,
    /// Complete changes, bounded again by one MiB of decoded bytes.
    #[garde(length(min = 1, max = 32), dive)]
    pub changes: Vec<TaskFileChange>,
}

/// Native publication state, retained independently of a runner response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEffectState {
    /// Durable intent, with no authorized ref mutation yet.
    Prepared,
    /// The native publisher may have issued its single ref mutation.
    Issued,
    /// GitHub commit, parent and tested tree have been verified.
    Confirmed,
    /// A remote write needs authoritative reconciliation.
    Unknown,
    /// Native preflight or remote history proved this operation could not publish.
    Refused,
}

impl TaskEffectState {
    /// Native stored spelling matches this shared wire vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Issued => "issued",
            Self::Confirmed => "confirmed",
            Self::Unknown => "unknown",
            Self::Refused => "refused",
        }
    }
}

/// No token or arbitrary remote response is exposed in this receipt.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEffect {
    /// Native effect `UUIDv7` discoverable after replacement-worker recovery.
    pub effect_id: String,
    /// Task-local publication order, independent of response delivery.
    pub ordinal: u64,
    /// Native state.
    pub state: TaskEffectState,
    /// Recorded expected branch head.
    pub expected_head: String,
    /// Verified candidate tree.
    pub tested_tree: String,
    /// Confirmed commit only after native readback.
    pub commit: Option<String>,
    /// Exact accepted PR URL.
    pub pull_request_url: String,
}

/// Fenced task-local effect pagination after a saved recovery watermark.
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskEffectsQuery {
    /// Current lease fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Selected current task.
    #[garde(length(bytes, min = 36, max = 36))]
    pub task_id: String,
    /// Checkpoint's last confirmed effect.
    #[serde(default)]
    #[garde(skip)]
    pub ordinal_after: u64,
    /// Last native ordinal returned by the preceding page.
    #[garde(inner(length(bytes, min = 1, max = 128)))]
    pub starting_after: Option<String>,
    /// Maximum effects in one response.
    #[serde(default = "default_limit")]
    #[garde(range(min = 1, max = 50))]
    pub limit: u32,
}

/// Ordered complete effect recovery page.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskEffectsPage {
    /// Includes unresolved and newly confirmed publications.
    pub effects: Vec<TaskEffect>,
    /// Native ordinal cursor when more rows remain.
    pub next_cursor: Option<String>,
}

/// Current lease proof for an individual publication read.
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskEffectQuery {
    /// Current fleet-slot fence.
    #[garde(skip)]
    pub fencing_token: u64,
}

const fn default_limit() -> u32 {
    50
}

fn file_path<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() > 32
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || parts
            .iter()
            .any(|part| matches!(*part, "" | "." | "..") || part.eq_ignore_ascii_case(".git"))
        || parts
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case(".gitmodules"))
        || parts
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case(".github"))
            && parts
                .get(1)
                .is_some_and(|part| part.eq_ignore_ascii_case("workflows"))
    {
        return Err(garde::Error::new("unsupported publication path"));
    }
    Ok(())
}
