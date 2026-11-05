//! Lease-scoped human input reads are separate from durable consumption receipts.

use garde::Validate;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, utoipa::IntoParams))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskInputsQuery {
    /// Current native lease fence.
    #[garde(range(max = i64::MAX as u64))]
    pub fencing_token: u64,
    /// Opaque ascending boundary returned by the previous input page.
    #[garde(inner(length(bytes, max = 1024)))]
    pub starting_after: Option<String>,
    /// Task-local position already present in this worker's recovered/current history.
    #[serde(default)]
    #[garde(inner(range(max = i64::MAX as u64)))]
    pub after_input_sequence: Option<u64>,
    /// At most four complete messages per safe boundary.
    #[serde(default = "input_page_size")]
    #[garde(range(min = 1, max = 4))]
    pub limit: u32,
}

const fn input_page_size() -> u32 { 4 }

/// One complete persisted human message; reading never consumes it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHumanInput {
    /// Native source admission identity used for decision and consumption.
    pub source_admission_id: String,
    /// Native FIFO position, independent of task-local input positions.
    pub sequence: u64,
    /// Complete user text; never a truncated interpretation.
    pub message: String,
    /// Already accepted meaning; recovery must answer it without reclassification.
    pub accepted: Option<TaskAcceptedInterpretation>,
}

/// Trusted compact work offered for intent association without loading private evidence.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInputCandidate {
    /// Native task identity.
    pub task_id: String,
    /// User-facing compact objective name.
    pub title: String,
    /// Current durable state.
    pub state: super::TaskState,
    /// Current native generation, required for an accepted steer.
    pub generation: u64,
}

/// Complete bounded messages and eligible current work from the same lease scope.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInputsResponse<'a> {
    /// Ascending native message order.
    pub inputs: Vec<TaskHumanInput>,
    /// Accepted meaning not yet incorporated by the current native task cursor.
    pub accepted_inputs: Vec<TaskAcceptedInput>,
    /// Current native objective and recovery pointer, including after interpretation replay.
    #[serde(borrow)]
    pub current_task: Option<super::TaskContext<'a>>,
    /// Current admission's complete saved meaning, including a conversation-only replay.
    pub current_interpretation: Option<TaskAcceptedInterpretation>,
    /// At most sixteen unfinished native objectives.
    pub tasks: Vec<TaskInputCandidate>,
    /// Absent when no next eligible input exists.
    pub next_cursor: Option<String>,
}

impl TaskInputsResponse<'_> {
    /// Retain native recovery state after the private HTTP response is wiped.
    #[must_use]
    pub fn into_owned(self) -> TaskInputsResponse<'static> {
        TaskInputsResponse {
            inputs: self.inputs,
            accepted_inputs: self.accepted_inputs,
            current_task: self.current_task.map(super::TaskContext::into_owned),
            current_interpretation: self.current_interpretation,
            tasks: self.tasks,
            next_cursor: self.next_cursor,
        }
    }
}

/// Complete immutable source interpretation, retained independently of a worker.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAcceptedInterpretation {
    /// Original admitted source.
    pub source_admission_id: String,
    /// Complete accepted plan digest.
    pub plan_digest: String,
    /// Complete original human text.
    pub message: String,
    /// Native item identities in the accepted original order.
    pub items: Vec<TaskRecoveredItem>,
}

/// An accepted item and its already confirmed original reply, if any.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecoveredItem {
    /// Native current association; no new execution is granted by reading it.
    pub binding: super::TaskItemBinding,
    /// Exact accepted byte slice.
    pub accepted_text: String,
    /// Immutable accepted interpretation.
    pub interpretation: String,
    /// Confirmed original response; absent means the item still needs an answer.
    pub response_text: Option<String>,
}

/// Recovery receipt for an acknowledged interpretation whose worker was lost.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAcceptedInput {
    /// Original native admission, retained even after queue settlement.
    pub source_admission_id: String,
    /// Immutable complete interpretation digest.
    pub plan_digest: String,
    /// Immutable interpreted item.
    pub item_id: String,
    /// Current task and generation that may apply this input.
    pub task: super::DecisionTaskVersion,
    /// Strict task-local input order.
    pub sequence: u64,
    /// Accepted native action.
    pub action: super::TaskInputAction,
    /// Complete original human message, preserving mixed-request context.
    pub original_message: String,
    /// Exact accepted source slice.
    pub accepted_text: String,
    /// Accepted interpretation, never recomputed after recovery.
    pub interpretation: String,
    /// Already incorporated natively; recovery restores meaning without applying it twice.
    pub applied: bool,
    /// Confirmed original answer when available, independent of the lost local history.
    pub response_text: Option<String>,
}
