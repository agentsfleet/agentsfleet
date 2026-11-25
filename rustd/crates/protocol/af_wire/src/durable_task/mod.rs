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
mod statement;
mod target;

pub use assessment::{DecisionAssessment, DecisionRetryDelay, DecisionUnavailableCause};
pub use context::DurableTaskContext;
pub use decision::{
    DecisionCallState, DecisionDispatch, DecisionDurableTaskVersion, DecisionNotSentCause,
    DecisionReceipt, DecisionReceiptInput, DecisionReceiptResult, DecisionRecordDisposition,
    DecisionRecorded, DecisionReservationInput, DecisionReservationResult,
};
pub use effect::{
    DurableTaskEffect, DurableTaskEffectInput, DurableTaskEffectQuery, DurableTaskEffectState,
    DurableTaskEffectsPage, DurableTaskEffectsQuery, DurableTaskFileChange,
};
pub use input::{
    DurableTaskAcceptedInput, DurableTaskAcceptedInterpretation, DurableTaskHumanInput,
    DurableTaskInputCandidate, DurableTaskInputsQuery, DurableTaskInputsResponse,
    DurableTaskRecoveredItem,
};
pub use target::{DurableTaskTarget, DurableTaskWorkflow};
mod dependency;
pub use binding::{
    DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskBindingsResponse,
    DurableTaskInputAction, DurableTaskItemBinding, DurableTaskRequestedTarget,
};
pub use checkpoint::*;
pub use dependency::{DurableTaskCheck, DurableTaskDependency};
pub use input_receipt::{
    DurableTaskInputMode, DurableTaskInputReceiptInput, DurableTaskInputReceiptResult,
};
pub use limits::{DurableTaskLimits, DurableTaskUsage};
pub use report::{DurableTaskDisposition, DurableTaskReport};
pub use state::DurableTaskState;
pub use statement::{DurableTaskStatement, STATEMENT_ID_MAX_BYTES, unique_statement_ids};

/// Linux's `PATH_MAX` (`linux/limits.h`): the longest path a system call
/// takes. A checkpointed or published path names a file in a Linux
/// workspace, so the operating system refuses anything longer.
pub(crate) const PATH_MAX_BYTES: usize = 4096;
/// Runner capability required before task-bound work may be issued.
pub const CAPABILITY: &str = "durable_tasks_v1";
