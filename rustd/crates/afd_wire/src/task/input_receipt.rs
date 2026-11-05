//! Safe-boundary application and source-message reply obligations are separate.

use garde::Validate;
use serde::{Deserialize, Serialize};
use super::DecisionTaskVersion;

/// One durable item can be applied once and answered once.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskInputMode {
    /// Current execution has included the accepted input in its next action group.
    ApplyHere,
    /// Native queued-work acknowledgement, with no other task execution authority.
    Queued,
    /// Conversational answer associated with its own original source message.
    Answered,
}

impl TaskInputMode {
    /// Persisted receipt identity.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self { Self::ApplyHere => "apply_here", Self::Queued => "queued", Self::Answered => "answered" }
    }
}

/// Record a native input outcome, without reinterpreting any accepted item.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskInputReceiptInput {
    /// Current slot fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Stable acknowledgement identity.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub operation_id: String,
    /// Accepted native source, never an invented reply destination.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub source_admission_id: String,
    /// Complete immutable interpretation identity.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]{64}$"))]
    pub plan_digest: String,
    /// One native action within the complete accepted plan.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub item_id: String,
    /// Expected associated task; absent only for conversation or clarification.
    #[garde(dive)]
    pub task: Option<DecisionTaskVersion>,
    /// Native application or answer mode.
    #[garde(skip)]
    pub mode: TaskInputMode,
    /// Required only for a conversational answer; queued text is formed natively.
    #[garde(inner(length(bytes, min = 1, max = 16_384), custom(afd_validate::nul_free)))]
    pub response_text: Option<String>,
}

/// Acknowledges history and cursors, without claiming a task goal is complete.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskInputReceiptResult {
    /// Accepted native item.
    pub item_id: String,
    /// Committed historical outcome.
    pub mode: TaskInputMode,
    /// Current covered input boundary when a task is associated.
    pub covered_input_sequence: Option<u64>,
    /// No repeated application, response or continuation was created.
    pub replayed: bool,
}
