//! Bounded public progress facts; no prompts, credentials or provider reasoning.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// The operation whose progress is being shown.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskActivityOperation {
    /// Interpret an authorized person's current message.
    Intent,
    /// Assess the accepted task goal after a complete action group.
    Goal,
    /// Save frozen files and private conversation.
    Capture,
    /// Restore authenticated files and private conversation.
    Restore,
}

/// A public stage, separate from authoritative task completion.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskActivityPhase {
    /// Work has begun; no successful outcome is implied.
    Starting,
    /// The original decision receipt was recorded.
    Recorded,
    /// No usable decision was established.
    Unavailable,
    /// Frozen contents are being inventoried.
    Preparing,
    /// Missing encrypted objects are being sent.
    Uploading,
    /// Native integrity verification is in progress.
    Verifying,
    /// Native manifest commit confirmed a saved recovery boundary.
    Saved,
    /// Authenticated contents are being restored into a fresh workspace.
    Restoring,
    /// All files and conversation have been restored.
    Restored,
    /// The operation failed; no saved/restored outcome is implied.
    Failed,
}

/// Progress comes from native identities and counters, never model prose.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskActivity {
    /// Physical decision or checkpoint operation identity.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))]
    pub activity_id: String,
    /// Native task, absent during initial intent routing.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")))]
    pub task_id: Option<String>,
    /// Exact original admission, when assessing human intent.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")))]
    pub source_admission_id: Option<String>,
    /// Native operation category.
    #[garde(skip)]
    pub operation: TaskActivityOperation,
    /// The current public stage.
    #[garde(skip)]
    pub phase: TaskActivityPhase,
    /// Selected provider label; no credential or URL.
    #[garde(inner(length(bytes, min = 1, max = 128)))]
    pub provider: Option<String>,
    /// Selected model label.
    #[garde(inner(length(bytes, min = 1, max = 256)))]
    pub model: Option<String>,
    /// Completed objects, never an inferred success percentage.
    #[garde(range(max = 200_000))]
    pub completed: u32,
    /// Total objects when known.
    #[garde(inner(range(max = 200_000)))]
    pub total: Option<u32>,
}

/// Saved progress uses the same discriminators as live activity.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "progress", rename_all = "snake_case")]
pub enum TaskActivityRecord {
    /// A physical judgment started.
    TaskDecisionStarted(TaskActivity),
    /// Its original receipt was recorded or remained unusable.
    TaskDecisionCompleted(TaskActivity),
    /// A bounded capture/restore stage advanced.
    TaskTransferProgress(TaskActivity),
}

/// Retained activity is bounded independently of the task's private history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskActivityHistory {
    /// Latest 128 operation/stage facts, in observed order.
    pub items: Vec<TaskActivityRecord>,
    /// Whether earlier stages were omitted by the public history bound.
    pub truncated: bool,
}
