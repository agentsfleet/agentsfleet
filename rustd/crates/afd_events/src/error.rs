//! What this crate refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, composed with
//! `#[from]` so `?` lifts a datastore or a queue failure without restating it.
//! Nothing here maps another crate's error to a string: the `source()` chain is
//! what an operator follows from "the history would not load" to the Postgres
//! detail that says why.

use afd_core::error_code::{self, ErrorCode};

/// Every way reading or tailing the narrative log can fail.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A statement would not run.
    #[error("the event store could not {context}")]
    Query {
        /// What was being attempted, for the operator's log line.
        context: &'static str,
        /// The Postgres failure underneath.
        #[source]
        source: sqlx::Error,
    },

    /// A row came back in a shape this build cannot read.
    #[error("core.fleet_events.{column} is not readable")]
    RowMalformed {
        /// Which column refused.
        column: &'static str,
        /// The decode failure underneath.
        #[source]
        source: sqlx::Error,
    },

    /// The caller's cursor is not one this daemon minted.
    ///
    /// Carries no source: nothing failed underneath, the bytes were simply not
    /// a cursor. A variant holding only data has no cause, and inventing one
    /// would put a Postgres error on a path Postgres never saw.
    #[error("the cursor is not one this daemon issued")]
    CursorMalformed,

    /// The pool would not give a connection.
    #[error("the event store's datastore is unavailable")]
    Datastore {
        /// The pool failure underneath.
        #[from]
        source: afd_db::Error,
    },

    /// The queue would not take an append, or would not be subscribed to.
    ///
    /// Distinct from [`Error::Datastore`] because the remedies differ: a steer
    /// Postgres accepted but the queue refused is a message a person sent and
    /// no runner will see.
    #[error("the fleet's stream could not be reached")]
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
            Self::CursorMalformed => DETAIL_CURSOR,
            Self::Datastore { .. } | Self::Queue { .. } => DETAIL_UNAVAILABLE,
        }
    }

    /// Whether the datastore or queue behind this crate could not be reached.
    ///
    /// An outage answers 503, where a statement that would not run is a 500.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(self, Self::Datastore { .. } | Self::Queue { .. })
    }

    /// The registry code a caller is refused with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Query { .. } | Self::RowMalformed { .. } => error_code::INTERNAL_DB_QUERY,
            Self::CursorMalformed => error_code::INVALID_REQUEST,
            Self::Datastore { .. } | Self::Queue { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
        }
    }
}

/// The sentence a statement that would not run earns.
///
/// `internalDbError`'s, verbatim. This crate would rather say which surface
/// failed, and that is exactly the change a client watching for
/// `UZ-INTERNAL-002` / "Database error" would feel: the pair is what the
/// daemon in production answers when a fleet-events statement refuses, and
/// `docs/REST_API_DESIGN_GUIDELINES.md` §9 makes re-spelling it inside `/v1`
/// a breaking change rather than an improvement.
const DETAIL_OPERATION_FAILED: &str = "Database error";

/// The sentence a cursor this daemon did not mint earns.
const DETAIL_CURSOR: &str = "The cursor is not valid";

/// The sentence an unreachable datastore or queue earns.
const DETAIL_UNAVAILABLE: &str = "Database unavailable";

/// This crate's result, defaulting to its own error.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::Query { context, source }
}

/// Reports a column this build cannot read.
pub(crate) fn row_malformed(column: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::RowMalformed { column, source }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
