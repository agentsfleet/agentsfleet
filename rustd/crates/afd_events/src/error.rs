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

    /// `since` and `cursor` were both supplied.
    ///
    /// They answer the same question two ways — one names a moment, the other
    /// names a row — and honouring both means guessing which the caller meant.
    #[error("since and cursor are mutually exclusive")]
    WindowAmbiguous,

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
            Self::WindowAmbiguous => DETAIL_WINDOW,
            Self::Datastore { .. } | Self::Queue { .. } => DETAIL_UNAVAILABLE,
        }
    }

    /// Whether the caller can fix this by sending a different request.
    ///
    /// The question the HTTP edge turns on first: a bad cursor is the client's
    /// to correct and answers 400, where everything else is this instance's
    /// problem.
    #[must_use]
    pub const fn is_client_fault(&self) -> bool {
        matches!(self, Self::CursorMalformed | Self::WindowAmbiguous)
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
            Self::Query { .. } | Self::RowMalformed { .. } => error_code::INTERNAL_OPERATION_FAILED,
            Self::CursorMalformed | Self::WindowAmbiguous => error_code::INVALID_REQUEST,
            Self::Datastore { .. } | Self::Queue { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
        }
    }
}

/// The sentence a statement that would not run earns.
const DETAIL_OPERATION_FAILED: &str = "The event history could not be read";

/// The sentence a cursor this daemon did not mint earns.
const DETAIL_CURSOR: &str = "The cursor is not valid";

/// The sentence supplying both window forms earns.
const DETAIL_WINDOW: &str = "since and cursor are mutually exclusive";

/// The sentence an unreachable datastore or queue earns.
const DETAIL_UNAVAILABLE: &str = "Service temporarily unavailable";

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
