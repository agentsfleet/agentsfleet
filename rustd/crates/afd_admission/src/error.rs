//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_ingress::Error`] and `afd_db::Error`
//! (`M-ERRORS-CANONICAL-STRUCTS`, with the workspace's declared divergence): a
//! struct carrying a captured backtrace over a private kind, with the code and
//! the sentence decided together in one table rather than spelled at each
//! raise site. The hull is `afd_core::error_shell!`, so nothing here repeats
//! what fifteen sibling crates already share.
//!
//! # What is deliberately NOT an error
//!
//! A queue that would not take an admission's append. The row is committed by
//! then, the caller is answered, and the replay sweeper appends when the queue
//! is back — see the crate header. [`ErrorKind::Queue`] exists for the replay
//! path, where a queue failure is the pass's outcome and not something a
//! producer is waiting on. Nor is a producer key reused with a different
//! payload: the key is the identity, the first payload stands, and the drift
//! is logged.

use afd_core::error::{
    DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE, DETAIL_OPERATION_FAILED,
};
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
    /// An admission failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The pool would not give a connection.
    ///
    /// The retryable refusal: nothing was committed, and a producer that
    /// retries later admits the same key as new work.
    #[error("the datastore holding the admission ledger would not answer")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// The queue would not take a replayed entry.
    #[error("the fleet's stream could not be reached")]
    Queue {
        #[source]
        source: afd_datastore::Error,
    },

    /// The entropy a row identifier is minted from could not be drawn.
    #[error("the entropy a row identifier is minted from could not be drawn")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },

    /// A row identifier could not be minted from the current instant.
    #[error("a row identifier could not be minted")]
    Identifier {
        #[source]
        source: afd_core::error::Error,
    },
}

impl Error {
    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                DETAIL_DATABASE_UNAVAILABLE,
            ),
            // A queue that is GONE is the same outage a caller retries
            // against, so it answers the unavailable code rather than a
            // generic 500. A queue that answered and refused is this
            // process's problem.
            ErrorKind::Queue { source } if source.is_unavailable() => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                DETAIL_DATABASE_UNAVAILABLE,
            ),
            ErrorKind::Query { .. } => (error_code::INTERNAL_DB_QUERY, DETAIL_DATABASE_ERROR),
            ErrorKind::Queue { .. } | ErrorKind::Entropy { .. } | ErrorKind::Identifier { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                DETAIL_OPERATION_FAILED,
            ),
        }
    }

    /// The registry code this failure answers with.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.answer().0
    }

    /// The sentence the caller is told — one of `afd_core::error`'s three,
    /// never a spelling of this crate's own.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }

    /// Whether a datastore behind this crate could not be reached.
    ///
    /// The question the HTTP edge turns on: an outage is this instance's to
    /// report as a 503, where every other failure here is a 500 (RULE ECL).
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. } => true,
            ErrorKind::Queue { source } => source.is_unavailable(),
            _reachable => false,
        }
    }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
