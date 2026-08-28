//! What this crate refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, composed with
//! `#[from]` so `?` lifts a datastore or a queue failure without restating it.
//! Nothing here maps another crate's error to a string: the `source()` chain is
//! what an operator follows from "the decision did not land" to the Postgres
//! detail that says why.

use afd_core::error_code::{self, ErrorCode};

/// Every way answering, reading or expiring a gate can fail.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A statement would not run.
    #[error("the approval store could not {context}")]
    Query {
        /// What was being attempted, for the operator's log line.
        context: &'static str,
        /// The Postgres failure underneath.
        #[source]
        source: sqlx::Error,
    },

    /// A row came back in a shape this build cannot read.
    #[error("core.fleet_approval_gates.{column} is not readable")]
    RowMalformed {
        /// Which column refused.
        column: &'static str,
        /// The decode failure underneath.
        #[source]
        source: sqlx::Error,
    },

    /// The pool would not give a connection.
    #[error("the approval store's datastore is unavailable")]
    Datastore {
        /// The pool failure underneath.
        #[from]
        source: afd_db::Error,
    },

    /// The queue would not take the continuation.
    ///
    /// Distinct from [`Error::Datastore`] because the remedies differ: a gate
    /// answered but not continued is a run a person unblocked and nothing
    /// restarted, which an operator resolves by retrying the decision.
    #[error("the continuation could not be appended")]
    Queue {
        /// The queue failure underneath.
        #[from]
        source: afd_redis::Error,
    },
}

impl Error {
    /// The sentence a caller is told.
    ///
    /// Static, and never the `source()` chain: an operator reads the chain in
    /// the log, and a caller who could read it would learn which statement this
    /// daemon runs.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self {
            Self::Query { .. } | Self::RowMalformed { .. } => DETAIL_OPERATION_FAILED,
            Self::Datastore { .. } | Self::Queue { .. } => DETAIL_UNAVAILABLE,
        }
    }

    /// Whether the datastore or queue behind this crate could not be reached.
    ///
    /// The question the HTTP edge turns on: an outage is this instance's
    /// problem and answers 503, where a statement that would not run is a 500.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(self, Self::Datastore { .. } | Self::Queue { .. })
    }

    /// The registry code a caller is refused with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Query { .. } | Self::RowMalformed { .. } => error_code::INTERNAL_OPERATION_FAILED,
            Self::Datastore { .. } | Self::Queue { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
        }
    }
}

/// The sentence a statement that would not run earns.
const DETAIL_OPERATION_FAILED: &str = "The approval could not be read or recorded";

/// The sentence an unreachable datastore or queue earns.
const DETAIL_UNAVAILABLE: &str = "Database unavailable";

/// This crate's result, defaulting to its own error.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::Query { context, source }
}

/// Reports a column this build cannot read.
///
/// Held for the readers that name a column rather than a statement; the page
/// and detail reads go through [`query`] because a `try_get` failure already
/// names the column and the type it refused.
#[expect(dead_code, reason = "the reads name their statement, not their column")]
pub(crate) fn row_malformed(column: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::RowMalformed { column, source }
}
