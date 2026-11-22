//! Prepared action meaning is distinct from an optional new objective owner.

use super::PreparedDurableTask;
use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskInputAction;
use std::ops::Range;

#[derive(Debug, Clone)]
pub(super) enum PreparedItem {
    New {
        task: PreparedDurableTask,
        interpretation: String,
    },
    Existing {
        source_span: Range<i32>,
        action: DurableTaskInputAction,
        durable_task_id: Option<Uuid7>,
        interpretation: String,
    },
}

impl PreparedItem {
    pub(super) const fn span(&self) -> &Range<i32> {
        match self {
            Self::New { task, .. } => &task.source_span,
            Self::Existing { source_span, .. } => source_span,
        }
    }

    pub(super) const fn action(&self) -> DurableTaskInputAction {
        match self {
            Self::New { .. } => DurableTaskInputAction::NewWork,
            Self::Existing { action, .. } => *action,
        }
    }

    pub(super) const fn existing_durable_task(&self) -> Option<&Uuid7> {
        match self {
            Self::Existing {
                durable_task_id, ..
            } => durable_task_id.as_ref(),
            Self::New { .. } => None,
        }
    }

    pub(super) fn interpretation(&self) -> &str {
        match self {
            Self::New { interpretation, .. } | Self::Existing { interpretation, .. } => {
                interpretation
            }
        }
    }
}
