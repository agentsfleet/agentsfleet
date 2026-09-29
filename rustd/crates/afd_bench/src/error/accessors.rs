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
            ErrorKind::DatabaseUnavailable { .. } | ErrorKind::StatementsUnreadable { .. } => {
                error_code::INTERNAL_DB_UNAVAILABLE
            }
            ErrorKind::FixtureUnseedable { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::QueueUnavailable { .. } => error_code::STARTUP_DRAGONFLY_CONNECT,
            _ if self.is_pre_flight() => error_code::STARTUP_ENV_CHECK,
            _ => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// Whether the statement counter would not answer, which almost always
    /// means Postgres started without `pg_stat_statements` preloaded.
    #[must_use]
    pub const fn is_statements_unreadable(&self) -> bool {
        matches!(self.kind(), ErrorKind::StatementsUnreadable { .. })
    }

    /// Whether the drain refused because another platform default already
    /// holds the provider it stages.
    #[must_use]
    pub const fn is_platform_default_held(&self) -> bool {
        matches!(self.kind(), ErrorKind::PlatformDefaultHeld { .. })
    }
}
