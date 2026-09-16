//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_ingress::Error`] and `afd_db::Error`
//! (`M-ERRORS-CANONICAL-STRUCTS`, with the workspace's declared divergence): a
//! struct carrying a captured backtrace over a private kind, with the code and
//! the sentence decided together in one table rather than spelled at each
//! raise site. The hull is `afd_core::error_shell!`, so nothing here repeats
//! what its sibling crates already share.
//!
//! # A gate answered but not continued is its own incident
//!
//! [`ErrorKind::Admission`] is distinct from [`ErrorKind::Datastore`] because
//! the remedies differ: a decision Postgres recorded whose continuation would
//! not admit is a run a person unblocked and nothing restarted, which an
//! operator resolves by retrying the decision.

use afd_core::error::DETAIL_DATABASE_UNAVAILABLE;
use afd_core::error_code::{self, ErrorCode};

mod raise;

#[cfg(feature = "test-util")]
pub use self::raise::one_of_each_kind;
pub(crate) use self::raise::query;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// An approval failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// A row came back in a shape this build cannot read.
    #[error("core.fleet_approval_gates.{column} is not readable")]
    RowMalformed {
        column: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// The pool would not give a connection.
    #[error("the approval store's datastore is unavailable")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    /// The entropy source would not answer.
    ///
    /// Its own variant rather than folded into [`ErrorKind::Datastore`]: a
    /// machine that cannot draw random bytes is not a machine whose Postgres
    /// is down, and an operator reading the two the same way would restart
    /// the wrong thing.
    #[error("an identifier could not be drawn")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },

    /// An identifier could not be minted from the instant it was drawn at.
    #[error("an identifier could not be minted")]
    Identifier {
        #[source]
        source: afd_core::error::Error,
    },

    /// The continuation's acceptance could not be recorded — see the module
    /// note.
    #[error("the continuation could not be admitted")]
    Admission {
        #[source]
        source: afd_admission::Error,
    },
}

impl Error {
    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Identifier { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                DETAIL_OPERATION_FAILED,
            ),
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                DETAIL_DATABASE_UNAVAILABLE,
            ),
            // The ledger already decided what a caller is told, and answering
            // a second sentence for one condition is the drift the shared
            // constants in `afd_core::error` exist to prevent.
            ErrorKind::Admission { source } => (source.code(), source.detail()),
        }
    }

    /// The registry code a caller is refused with.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.answer().0
    }

    /// The sentence a caller is told.
    ///
    /// Static, and never the `source()` chain: an operator reads the chain in
    /// the log, and a caller who could read it would learn which statement
    /// this daemon runs.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }

    /// Whether a datastore behind this crate could not be reached.
    ///
    /// The question the HTTP edge turns on: an outage is this instance's
    /// problem and answers 503, where a statement that would not run is a 500.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. } => true,
            ErrorKind::Admission { source } => source.is_datastore_unavailable(),
            _reachable => false,
        }
    }
}

/// The sentence a statement that would not run earns.
///
/// This crate's own rather than one of `afd_core::error`'s three: an approval
/// is a thing a PERSON is waiting on, and "Database error" tells them nothing
/// about the decision they just made.
const DETAIL_OPERATION_FAILED: &str = "The approval could not be read or recorded";

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
