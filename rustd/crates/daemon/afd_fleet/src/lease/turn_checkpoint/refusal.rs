//! Fieldless recovery outcomes compose into the fleet plane's existing error shell.

use af_core::error_code::{self, ErrorCode};

use crate::error::{Error, ErrorKind};

/// Recovery never substitutes a fresh run after an incompatible saved state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnCheckpointRefusal {
    /// The root, files or compatibility witnesses cannot safely be restored.
    Incompatible,
    /// A stable operation or expected head has different saved content.
    Conflict,
    /// Checkpoint custody cannot currently be established.
    Unavailable,
    /// A reviewed size, quota or retained-generation bound was reached.
    Limit,
}

impl TurnCheckpointRefusal {
    /// Stable public code, independent of private state or provider data.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::Incompatible => error_code::RUN_CHECKPOINT_INCOMPATIBLE,
            Self::Conflict => error_code::RUN_CHECKPOINT_CONFLICT,
            Self::Unavailable => error_code::RUN_CHECKPOINT_UNAVAILABLE,
            Self::Limit => error_code::RUN_CHECKPOINT_LIMIT,
        }
    }

    /// Public sentence that never quotes saved history or object material.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Incompatible => "the checkpoint cannot safely restore this turn",
            Self::Conflict => "the checkpoint operation or expected head changed",
            Self::Unavailable => "checkpoint custody cannot currently be established",
            Self::Limit => "the checkpoint exceeds a size, storage or retention limit",
        }
    }
}

impl std::fmt::Display for TurnCheckpointRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.detail())
    }
}

impl From<TurnCheckpointRefusal> for Error {
    fn from(reason: TurnCheckpointRefusal) -> Self {
        Self::from(ErrorKind::TurnCheckpointRefused { reason })
    }
}
