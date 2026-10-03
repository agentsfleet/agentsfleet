//! What a provider fails with.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull. A provider failure ends the run, and how the
//! report names it depends on which failure it was: a refusal no retry changes
//! is the fleet's error, with the status in its detail; a connection lost
//! mid-turn is a transport loss.
//!
//! # Which code, and why none is new
//!
//! An operator reads a provider failure on the host's journal, so it reuses the
//! registry's internal code as the rest of the runner does; the log line's
//! `event` says which failure it was.

use afd_core::error_code::{self, ErrorCode};
use afd_wire::report::FailureClass;

afd_core::error_shell!(
    /// A provider failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way a provider fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The provider refused the turn with a status no retry changes.
    #[error("the model provider refused the turn with status {status}")]
    Refused {
        /// The HTTP status.
        status: u16,
    },

    /// The connection was lost before the turn ended.
    #[error("the connection to the model provider was lost mid-turn")]
    Lost {
        /// The transport's reason.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// A refusal with `status`, which no retry changes.
    #[must_use]
    pub fn refused(status: u16) -> Self {
        Self::from(ErrorKind::Refused { status })
    }

    /// A connection lost mid-turn, for `source`.
    #[must_use]
    pub fn lost(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::from(ErrorKind::Lost {
            source: source.into(),
        })
    }

    /// The failure in a sentence, without its code or backtrace, for a
    /// report's detail.
    #[must_use]
    pub fn detail(&self) -> String {
        self.kind().to_string()
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Refused { .. } | ErrorKind::Lost { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
        }
    }

    /// The class the report names, where the failure has one. A refusal is
    /// the fleet's error and carries none.
    #[must_use]
    pub fn failure_class(&self) -> Option<FailureClass> {
        match self.kind() {
            ErrorKind::Refused { .. } => None,
            ErrorKind::Lost { .. } => Some(FailureClass::TransportLoss),
        }
    }
}
