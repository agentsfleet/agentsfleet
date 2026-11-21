//! Durable objective context carried by the existing run protocol.
//!
//! A task is an owner across runs. Its generation, a run fence and a
//! checkpoint generation remain separate counters with separate purposes.

mod assessment;
mod binding;
mod checkpoint;
mod context;
mod decision;
mod effect;
mod input;
mod input_receipt;
mod limits;
mod report;
mod state;
mod target;

pub use assessment::{DecisionAssessment, DecisionRetryDelay, DecisionUnavailableCause};
pub use context::TaskContext;
pub use decision::{
    DecisionCallState, DecisionDispatch, DecisionNotSentCause, DecisionReceipt,
    DecisionReceiptInput, DecisionReceiptResult, DecisionRecordDisposition, DecisionRecorded,
    DecisionReservationInput, DecisionReservationResult, DecisionTaskVersion,
};
pub use effect::{
    TaskEffect, TaskEffectInput, TaskEffectQuery, TaskEffectState, TaskEffectsPage,
    TaskEffectsQuery, TaskFileChange,
};
pub use input::{
    TaskAcceptedInput, TaskAcceptedInterpretation, TaskHumanInput, TaskInputCandidate,
    TaskInputsQuery, TaskInputsResponse, TaskRecoveredItem,
};
pub use target::{TaskTarget, TaskWorkflow};
mod dependency;
pub use binding::{
    TaskBindingItem, TaskBindingsInput, TaskBindingsResponse, TaskInputAction, TaskItemBinding,
    TaskProposedItem, TaskRequestedTarget,
};
pub use checkpoint::*;
pub use dependency::{TaskCheck, TaskDependency};
pub use input_receipt::{TaskInputMode, TaskInputReceiptInput, TaskInputReceiptResult};
pub use limits::{TaskLimits, TaskUsage};
pub use report::{TaskDisposition, TaskReport};
pub use state::TaskState;

/// Linux's `PATH_MAX` (`linux/limits.h`): the longest path a system call
/// takes. A checkpointed or published path names a file in a Linux
/// workspace, so the operating system refuses anything longer.
pub(crate) const PATH_MAX_BYTES: usize = 4096;
/// Runner capability required before task-bound work may be issued.
pub const CAPABILITY: &str = "durable_tasks_v1";
