//! Fieldless lifecycle vocabulary; the crate's existing error shell composes it.
//!
//! These carry no causal source or allocation. They identify normal guarded
//! refusals, while database and transport faults retain their own causes.

use std::fmt;

use afd_core::error_code::{self, ErrorCode};

use crate::error::{Error, ErrorKind};

/// Why a durable objective cannot accept this operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRefusal {
    /// No owner is visible within the current native scope.
    NotFound,
    /// Generation, state or stopped ownership changed.
    StaleState,
    /// An immutable source/operation identity has different content.
    ReplayConflict,
    /// Native target evidence is missing or invalid.
    InvalidTarget,
    /// An admitted work or capacity allowance is exhausted.
    LimitExhausted,
    /// The saved profile, confirmed credit or platform credential is unavailable.
    DecisionUnavailable,
    /// A physical cost exceeded its reservation; related paid work is blocked.
    DecisionOverage,
    /// Remote acknowledgement does not prove whether the saved publication occurred.
    EffectUncertain,
    /// Only a complete compatible retained checkpoint can be attached.
    CheckpointIncompatible,
    /// Object custody cannot currently be established.
    StorageUnavailable,
    /// Intent, multiplicity or association does not meet the reviewed threshold.
    InputAmbiguous,
}

impl TaskRefusal {
    /// The fixed public code for this lifecycle outcome.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::NotFound => error_code::TASK_NOT_FOUND,
            Self::StaleState => error_code::TASK_STALE_STATE,
            Self::ReplayConflict => error_code::TASK_REPLAY_CONFLICT,
            Self::InvalidTarget => error_code::TASK_INVALID_TARGET,
            Self::LimitExhausted | Self::DecisionOverage => error_code::TASK_LIMIT_EXHAUSTED,
            Self::DecisionUnavailable => error_code::TASK_DECISION_UNAVAILABLE,
            Self::InputAmbiguous => error_code::TASK_INPUT_AMBIGUOUS,
            Self::EffectUncertain => error_code::TASK_EFFECT_UNCERTAIN,
            Self::CheckpointIncompatible => error_code::TASK_CHECKPOINT_INCOMPATIBLE,
            Self::StorageUnavailable => error_code::TASK_STORAGE_UNAVAILABLE,
        }
    }

    /// A public sentence without task contents or private target data.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::EffectUncertain => "the saved publication requires authoritative reconciliation",
            Self::InputAmbiguous => "the input needs clarification before it can change work",
            Self::CheckpointIncompatible => "the checkpoint cannot safely restore this task generation",
            Self::StorageUnavailable => "checkpoint storage cannot currently establish object custody",
            Self::NotFound => "the task does not belong to this workspace and fleet",
            Self::StaleState => "the task generation or lifecycle state changed",
            Self::ReplayConflict => "the task operation conflicts with its saved interpretation",
            Self::InvalidTarget => "the task has no accepted native target binding",
            Self::LimitExhausted => "the task allowance or unfinished-task capacity is exhausted",
            Self::DecisionUnavailable => {
                "the fleet cannot currently obtain a funded reviewed decision"
            }
            Self::DecisionOverage => {
                "a decision cost exceeded its reservation; related paid work is blocked"
            }
        }
    }
}

impl fmt::Display for TaskRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}

impl From<TaskRefusal> for Error {
    fn from(reason: TaskRefusal) -> Self {
        Self::from(ErrorKind::TaskRefused { reason })
    }
}
