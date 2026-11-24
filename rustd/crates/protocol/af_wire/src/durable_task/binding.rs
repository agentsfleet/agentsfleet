//! A run's task tools bind one span of a person's message to durable work.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// What a bound item does to durable work.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableTaskInputAction {
    /// Apply a person's guidance to an existing task.
    Steer,
    /// Start a new task.
    NewWork,
}

impl DurableTaskInputAction {
    /// The stored `fleet.durable_task_items.action` spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Steer => "steer",
            Self::NewWork => "new_work",
        }
    }
}

/// Requested target; the daemon resolves its authority and exact remote identity.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DurableTaskRequestedTarget {
    /// API-only work has no mandatory repository checkout or GitHub grant.
    ApiOnly,
    /// Existing-PR work must name an exact human-assigned same-repository PR.
    ExistingPr {
        /// Repository inside the fleet's approved GitHub grant.
        #[garde(
            length(chars, min = 3, max = af_validate::TEXT_MAX_CHARS),
            pattern(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
        )]
        repository: String,
        /// Exact open PR requested by the person.
        #[garde(range(min = 1))]
        number: u32,
    },
}

/// One span of a person's message bound to durable work. It is a new task with
/// the statements it is judged done against, or a steer of an unfinished task.
///
/// Spans are byte offsets into the complete source message; the daemon checks
/// their UTF-8 boundaries and takes the text it accepts from them.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum DurableTaskBindingItem {
    /// Start a task whose objective is the span.
    NewWork {
        /// Inclusive byte offset in the complete original human message.
        #[garde(range(min = 0))]
        source_start: i32,
        /// Exclusive byte offset.
        #[garde(range(min = 1))]
        source_end: i32,
        /// Compact label; the accepted objective remains the source text.
        #[garde(
            length(chars, min = 1, max = af_validate::TEXT_MAX_CHARS),
            custom(af_validate::nul_free)
        )]
        title: String,
        /// What the task works on; the daemon resolves its authority.
        #[garde(dive)]
        target: DurableTaskRequestedTarget,
        /// At least one statement, each with a unique id.
        #[garde(length(min = 1), dive, custom(super::unique_statement_ids))]
        statements: Vec<super::DurableTaskStatement>,
    },
    /// Join the span to an unfinished task of the same fleet.
    Steer {
        /// Inclusive byte offset in the complete original human message.
        #[garde(range(min = 0))]
        source_start: i32,
        /// Exclusive byte offset.
        #[garde(range(min = 1))]
        source_end: i32,
        /// The unfinished task the span steers.
        #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
        durable_task_id: String,
    },
}

impl DurableTaskBindingItem {
    /// What the item does, as its stored `action` spells it.
    #[must_use]
    pub const fn action(&self) -> DurableTaskInputAction {
        match self {
            Self::NewWork { .. } => DurableTaskInputAction::NewWork,
            Self::Steer { .. } => DurableTaskInputAction::Steer,
        }
    }

    /// The item's byte span of the source message, start inclusive.
    #[must_use]
    pub const fn span(&self) -> (i32, i32) {
        match self {
            Self::NewWork {
                source_start,
                source_end,
                ..
            }
            | Self::Steer {
                source_start,
                source_end,
                ..
            } => (*source_start, *source_end),
        }
    }
}

/// Bind one span of the run's source message to durable work.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DurableTaskBindingsInput {
    /// Current thread fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// The person's message: the run's head turn or one folded into it.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub source_turn_id: String,
    /// SHA-256 of `interpretation()`, computed independently by the daemon.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]{64}$"))]
    pub plan_digest: String,
    /// The one item a tool call binds; a source turn takes one plan.
    #[garde(length(min = 1, max = 1), dive)]
    pub items: Vec<DurableTaskBindingItem>,
}

impl DurableTaskBindingsInput {
    /// Stable meaning excludes the run fence, which grants no replay authority.
    #[must_use]
    pub fn interpretation(&self) -> impl Serialize + '_ {
        (&self.source_turn_id, &self.items)
    }
}

/// One accepted immutable item and the task it bound.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableTaskItemBinding {
    /// Native immutable item `UUIDv7`.
    pub item_id: String,
    /// The task the item started or steers.
    pub durable_task_id: Option<String>,
    /// Current native generation, including a wake caused by this input.
    pub durable_task_generation: Option<u64>,
    /// What the item did.
    pub action: DurableTaskInputAction,
    /// Whether the current run may apply this input at its next safe boundary.
    pub applies_here: bool,
    /// Native FIFO cursor for an accepted existing-task input.
    pub input_sequence: Option<u64>,
}

/// Complete message receipt, stable across lost acknowledgements.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableTaskBindingsResponse<'a> {
    /// Accepted immutable items in their original order.
    pub items: Vec<DurableTaskItemBinding>,
    /// Current run objective after binding, with native target and recovery state.
    #[serde(borrow)]
    pub current_durable_task: Option<super::DurableTaskContext<'a>>,
    /// Current run's immutable ceiling after native task binding.
    #[serde(
        default,
        with = "crate::instant::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub execution_deadline: Option<jiff::Timestamp>,
    /// True when no new item, task, input or continuation was allocated.
    pub replayed: bool,
}

impl DurableTaskBindingsResponse<'_> {
    /// Keep the native acknowledgement without retaining the transport buffer.
    #[must_use]
    pub fn into_owned(self) -> DurableTaskBindingsResponse<'static> {
        DurableTaskBindingsResponse {
            items: self.items,
            current_durable_task: self
                .current_durable_task
                .map(super::DurableTaskContext::into_owned),
            execution_deadline: self.execution_deadline,
            replayed: self.replayed,
        }
    }
}
