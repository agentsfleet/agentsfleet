//! What one tool call's binding item became after its checks: a new task, or
//! a steer of an unfinished one.

use super::PreparedDurableTask;
use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskInputAction;
use std::ops::Range;

#[derive(Debug, Clone)]
pub(super) enum PreparedItem {
    /// A task whose objective is the span, judged against its own statements.
    New { task: PreparedDurableTask },
    /// The span, joined to an unfinished task of the same fleet.
    Steer {
        source_span: Range<i32>,
        durable_task_id: Uuid7,
    },
}

impl PreparedItem {
    pub(super) const fn span(&self) -> &Range<i32> {
        match self {
            Self::New { task } => &task.source_span,
            Self::Steer { source_span, .. } => source_span,
        }
    }

    pub(super) const fn action(&self) -> DurableTaskInputAction {
        match self {
            Self::New { .. } => DurableTaskInputAction::NewWork,
            Self::Steer { .. } => DurableTaskInputAction::Steer,
        }
    }
}
