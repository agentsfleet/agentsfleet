//! Why a memory call was refused.
//!
//! A refused entry carries the bound it broke, so the error takes the
//! `afd_core::error_shell!` hull. Both codes are the registry's request codes:
//! the model wrote the entry, and the daemon would have skipped it.

use afd_core::error_code::{self, ErrorCode};
use afd_wire::memory::MAX_PUSH_BYTES;

afd_core::error_shell!(
    /// A refused store, with the backtrace of where it was refused.
    pub struct Error(ErrorKind);
);

/// Every way a store is refused.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The entry breaks a bound the daemon declares on a stored entry.
    #[error("the entry breaks a stored bound")]
    Malformed {
        /// Which bound, per field.
        #[source]
        source: garde::Report,
    },

    /// The recall query could not be built into a matcher.
    #[error("the query cannot be searched for")]
    Query {
        /// The matcher's reason.
        #[source]
        source: aho_corasick::BuildError,
    },

    /// The run's stored entries would no longer fit one push.
    #[error("the run's stored memory would take {needed} bytes, past the push's {MAX_PUSH_BYTES}")]
    Full {
        /// The bytes the stored entries would take with this one.
        needed: usize,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this refusal carries.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Malformed { .. } | ErrorKind::Query { .. } => error_code::INVALID_REQUEST,
            ErrorKind::Full { .. } => error_code::PAYLOAD_TOO_LARGE,
        }
    }

    /// Whether the run's memory is full, rather than the entry malformed.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        matches!(self.kind(), ErrorKind::Full { .. })
    }

    /// What the model reads: each broken bound as `field: reason`, or how far
    /// past the push the store would go.
    #[must_use]
    pub fn detail(&self) -> String {
        match self.kind() {
            ErrorKind::Malformed { source } => source
                .iter()
                .map(|(path, broken)| format!("{path}: {broken}"))
                .collect::<Vec<_>>()
                .join("; "),
            other @ (ErrorKind::Query { .. } | ErrorKind::Full { .. }) => other.to_string(),
        }
    }
}

/// An entry breaking the bounds `report` names.
pub(crate) fn malformed(source: garde::Report) -> Error {
    ErrorKind::Malformed { source }.into()
}

/// A recall query the matcher refused.
pub(crate) fn query(source: aho_corasick::BuildError) -> Error {
    ErrorKind::Query { source }.into()
}

/// A store that would take the run's memory to `needed` bytes.
pub(crate) fn full(needed: usize) -> Error {
    ErrorKind::Full { needed }.into()
}
