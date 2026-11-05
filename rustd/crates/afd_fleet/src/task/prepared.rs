//! Prepared action meaning is distinct from an optional new objective owner.

use super::PreparedTask;
use afd_core::id::Uuid7;
use afd_wire::task::TaskInputAction;
use std::ops::Range;

#[derive(Debug, Clone)]
pub(super) enum PreparedItem {
    New {
        task: PreparedTask,
        interpretation: String,
    },
    Existing {
        source_span: Range<i32>,
        action: TaskInputAction,
        task_id: Option<Uuid7>,
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

    pub(super) const fn action(&self) -> TaskInputAction {
        match self {
            Self::New { .. } => TaskInputAction::NewWork,
            Self::Existing { action, .. } => *action,
        }
    }

    pub(super) fn existing_task(&self) -> Option<&Uuid7> {
        match self {
            Self::Existing { task_id, .. } => task_id.as_ref(),
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
