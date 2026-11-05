//! Complete interpreted messages retain their action spans and decision proofs.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// A closed routing answer; criteria remain authored in the accepted setup.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskInputAction {
    /// Respond conversationally without creating or changing an objective.
    Conversation,
    /// Explain the recorded state of an existing objective.
    Status,
    /// Request a fresh assessment of an existing objective.
    Refresh,
    /// Apply new human guidance to an existing objective.
    Steer,
    /// Propose a new objective under the accepted setup.
    NewWork,
    /// Ask for information needed to resolve the human request.
    Clarify,
}

impl TaskInputAction {
    /// Stable reviewed option identity used in decision questions and receipts.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Status => "status",
            Self::Refresh => "refresh",
            Self::Steer => "steer",
            Self::NewWork => "new_work",
            Self::Clarify => "clarify",
        }
    }
}

/// Requested target; the daemon resolves its authority and exact remote identity.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskRequestedTarget {
    /// API-only work has no mandatory repository checkout or GitHub grant.
    ApiOnly,
    /// Existing-PR work must name an exact human-assigned same-repository PR.
    ExistingPr {
        /// Repository inside both accepted setup and current grant ceiling.
        #[garde(
            length(bytes, min = 3, max = 255),
            pattern(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
        )]
        repository: String,
        /// Exact open PR requested by the person.
        #[garde(range(min = 1))]
        number: u32,
    },
}

/// Model-proposed meaning; its digest is frozen before item decision calls.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskProposedItem {
    /// Inclusive byte offset in the complete original human message.
    #[garde(range(min = 0))]
    pub source_start: i32,
    /// Exclusive byte offset; native code checks UTF-8 boundaries and non-overlap.
    #[garde(range(min = 1))]
    pub source_end: i32,
    /// One proposed route, subject to the recorded configured decision model.
    #[garde(skip)]
    pub action: TaskInputAction,
    /// Existing same-fleet task, when the item concerns prior work.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$")))]
    pub task_id: Option<String>,
    /// Compact label; the accepted objective remains the original source text.
    #[garde(length(bytes, min = 1, max = 160), custom(afd_validate::nul_free))]
    pub title: String,
    /// Bounded explanation retained in the native interpretation receipt.
    #[garde(length(bytes, min = 1, max = 16_384), custom(afd_validate::nul_free))]
    pub interpretation: String,
    /// Required for new work; not an authority token.
    #[garde(dive)]
    pub target: Option<TaskRequestedTarget>,
}

/// One action and its separate decision proof when decomposition is required.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskBindingItem {
    /// Complete item whose meaning was pinned before classification.
    #[garde(dive)]
    pub proposed: TaskProposedItem,
    /// Omit only for a single request already covered by the message decision.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$")))]
    pub decision_call_id: Option<String>,
}

/// Atomically accept the complete bounded interpretation of one admitted input.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskBindingsInput {
    /// Current slot fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Complete native source admission; queued human claims use this same ID.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub source_admission_id: String,
    /// Reviewed setup used for the message's questions and new objectives.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub accepted_revision_id: String,
    /// SHA-256 of `interpretation()`, computed independently by the daemon.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]{64}$"))]
    pub plan_digest: String,
    /// Paid, accounted, current complete-message routing decision.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub message_decision_call_id: String,
    /// The complete interpretation; no prefix can be partially accepted.
    #[garde(length(min = 1, max = 4), dive)]
    pub items: Vec<TaskBindingItem>,
}

impl TaskBindingsInput {
    /// Stable meaning excludes call IDs and lease fences, which grant no replay authority.
    #[must_use]
    pub fn interpretation(&self) -> impl Serialize + '_ {
        (
            &self.source_admission_id,
            &self.accepted_revision_id,
            self.items
                .iter()
                .map(|item| &item.proposed)
                .collect::<Vec<_>>(),
        )
    }
}

/// One accepted immutable action, with no invented task for conversational input.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskItemBinding {
    /// Native immutable item `UUIDv7`.
    pub item_id: String,
    /// Native objective, absent for conversation or clarification.
    pub task_id: Option<String>,
    /// Current native generation, including a wake caused by this input.
    pub task_generation: Option<u64>,
    /// Accepted routing meaning.
    pub action: TaskInputAction,
    /// Whether the current run may apply this input at its next safe boundary.
    pub applies_here: bool,
    /// Native FIFO cursor for an accepted existing-task input.
    pub input_sequence: Option<u64>,
}

/// Complete message receipt, stable across lost acknowledgements.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskBindingsResponse<'a> {
    /// Accepted immutable items in their original order.
    pub items: Vec<TaskItemBinding>,
    /// Current lease objective after binding, with native target and recovery state.
    #[serde(borrow)]
    pub current_task: Option<super::TaskContext<'a>>,
    /// Current lease's immutable ceiling after native task binding.
    #[serde(
        default,
        with = "crate::instant::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub execution_deadline: Option<jiff::Timestamp>,
    /// True when no new item, task, input or continuation was allocated.
    pub replayed: bool,
}

impl TaskBindingsResponse<'_> {
    /// Keep the native acknowledgement without retaining the transport buffer.
    #[must_use]
    pub fn into_owned(self) -> TaskBindingsResponse<'static> {
        TaskBindingsResponse {
            items: self.items,
            current_task: self.current_task.map(super::TaskContext::into_owned),
            execution_deadline: self.execution_deadline,
            replayed: self.replayed,
        }
    }
}
