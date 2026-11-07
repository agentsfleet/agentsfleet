//! The questions a caller may ask of a refusal, and the code each one answers.

use afd_core::error_code::{self, ErrorCode};

use super::{Error, ErrorKind};

impl Error {
    /// The registry code this refusal answers.
    ///
    /// Reused rather than minted; the module note on [`crate::error`] says why.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::DatabaseUnavailable { .. }
            | ErrorKind::StatementsUnreadable { .. }
            | ErrorKind::SamplerUnavailable { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::FixtureUnseedable { .. } | ErrorKind::FixtureRefused { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            ErrorKind::QueueUnavailable { .. } => error_code::STARTUP_DRAGONFLY_CONNECT,
            _ if self.is_pre_flight() => error_code::STARTUP_ENV_CHECK,
            _ => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}
