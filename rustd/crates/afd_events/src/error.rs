//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_ingress::Error`] and `afd_db::Error`
//! (`M-ERRORS-CANONICAL-STRUCTS`, with the workspace's declared divergence): a
//! struct carrying a captured backtrace over a private kind, with the code and
//! the sentence decided together in one table rather than spelled at each
//! raise site. The hull is `afd_core::error_shell!`, so nothing here repeats
//! what its sibling crates already share.
//!
//! # A cursor this daemon did not mint is not a failure of this daemon
//!
//! [`ErrorKind::CursorMalformed`] is the one caller fault here, and it carries
//! no source: nothing failed underneath, the bytes were simply not a cursor.
//! Inventing a cause for it would put a Postgres error on a path Postgres
//! never saw (`RUST_ERROR_STANDARD` rule 4's second half).
//!
//! # An admission answers for itself
//!
//! A steer whose ledger row would not commit is one this daemon has NOT
//! accepted, so the refusal travels. It answers the LEDGER's code and
//! sentence rather than a second spelling of the same condition — the drift
//! `afd_core::error`'s shared constants exist to prevent.

use afd_core::error::{DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE};
use afd_core::error_code::{self, ErrorCode};

mod raise;

#[cfg(feature = "test-util")]
pub use self::raise::one_of_each_kind;
pub(crate) use self::raise::{cursor_malformed, query, row_malformed};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// An event-log failure, with the backtrace of where it was raised.
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
    #[error("core.fleet_events.{column} is not readable")]
    RowMalformed {
        column: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// The caller's cursor is not one this daemon minted.
    #[error("the cursor is not one this daemon issued")]
    CursorMalformed,

    /// The pool would not give a connection.
    #[error("the event store's datastore is unavailable")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    /// The queue would not be subscribed to, or would not take a frame.
    #[error("the fleet's stream could not be reached")]
    Queue {
        #[source]
        source: afd_dragonfly::Error,
    },

    /// The acceptance could not be recorded.
    #[error("the message could not be admitted")]
    Admission {
        #[source]
        source: afd_admission::Error,
    },
}

impl Error {
    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => {
                (error_code::INTERNAL_DB_QUERY, DETAIL_DATABASE_ERROR)
            }
            ErrorKind::CursorMalformed => (error_code::INVALID_REQUEST, DETAIL_CURSOR),
            ErrorKind::Datastore { .. } | ErrorKind::Queue { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                DETAIL_DATABASE_UNAVAILABLE,
            ),
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

    /// Whether the datastore or queue behind this crate could not be reached.
    ///
    /// An outage answers 503, where a statement that would not run is a 500.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. } | ErrorKind::Queue { .. } => true,
            ErrorKind::Admission { source } => source.is_datastore_unavailable(),
            _reachable => false,
        }
    }

    /// Whether the pool itself would not answer.
    ///
    /// Narrower than [`Self::is_datastore_unavailable`], and the counters
    /// suite's question: a best-effort read that skipped because the pool was
    /// empty is a different fact from one the queue refused.
    #[must_use]
    pub fn is_pool_unavailable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Datastore { .. })
    }
}

/// The sentence a cursor this daemon did not mint earns.
///
/// This crate's own, because no other plane answers it: every sibling's
/// caller-fault sentence is about a different thing.
const DETAIL_CURSOR: &str = "The cursor is not valid";
