//! Durable objective context carried by the existing lease protocol.
//!
//! A task is an owner across leases. Its generation, a lease fence and a
//! checkpoint generation remain separate counters with separate purposes.

mod assessment;
mod context;
mod target;
mod decision;
mod limits;
mod input;
mod effect;
mod binding;
mod input_receipt;
mod checkpoint;
mod state;
mod report;

pub use assessment::{DecisionAssessment, DecisionRetryDelay, DecisionUnavailableCause};
pub use context::TaskContext;
pub use target::{TaskTarget, TaskWorkflow};
pub use decision::{
    DecisionCallState, DecisionDispatch, DecisionNotSentCause, DecisionReceipt,
    DecisionReceiptInput, DecisionReceiptResult, DecisionRecordDisposition, DecisionRecorded,
    DecisionReservationInput, DecisionReservationResult, DecisionTaskVersion,
};
pub use input::{TaskHumanInput, TaskInputCandidate, TaskInputsQuery, TaskInputsResponse, TaskAcceptedInput,
    TaskAcceptedInterpretation, TaskRecoveredItem};
pub use effect::{TaskEffect, TaskEffectInput, TaskEffectQuery, TaskEffectState, TaskEffectsPage, TaskEffectsQuery, TaskFileChange};
pub use binding::{TaskBindingItem, TaskBindingsInput, TaskBindingsResponse, TaskInputAction, TaskItemBinding, TaskProposedItem, TaskRequestedTarget};
pub use input_receipt::{TaskInputMode, TaskInputReceiptInput, TaskInputReceiptResult};
pub use checkpoint::*;
pub use limits::{TaskLimits, TaskUsage};
pub use state::TaskState;
pub use report::{TaskDisposition, TaskReport};

/// Maximum unfinished objectives a fleet can retain.
pub const UNFINISHED_MAX: u32 = 16;
/// Maximum human admissions read at one safe boundary.
pub const INPUT_BATCH_MAX: usize = 4;
/// Maximum immutable interpreted actions from one admitted message.
pub const ITEMS_MAX: usize = 4;
/// Maximum UTF-8 bytes in an accepted objective.
pub const OBJECTIVE_MAX_BYTES: usize = 16 * 1024;
/// Maximum UTF-8 bytes in a compact task title.
pub const TITLE_MAX_BYTES: usize = 160;
/// Maximum semantic completion questions accepted for one objective.
pub const QUESTIONS_MAX: usize = 6;
/// Maximum UTF-8 bytes in one completion question.
pub const QUESTION_MAX_BYTES: usize = 1024;
/// Runner capability required before task-bound work may be issued.
pub const CAPABILITY: &str = "durable_tasks_v1";
