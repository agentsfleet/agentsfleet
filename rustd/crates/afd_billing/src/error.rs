//! What this crate refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, composed with
//! `#[from]` so `?` lifts a datastore failure without restating it. Nothing
//! here maps another crate's error to a string: the `source()` chain is what an
//! operator follows from "the charge did not land" to the Postgres detail that
//! says why.

use afd_core::error_code::{self, ErrorCode};

/// Every way reading a balance, deciding a budget or recording a charge can
/// fail.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A statement would not run.
    #[error("the billing store could not {context}")]
    Query {
        /// What was being attempted, for the operator's log line.
        context: &'static str,
        /// The Postgres failure underneath.
        #[source]
        source: sqlx::Error,
    },

    /// A stored value is not a shape this daemon can read.
    ///
    /// Names the table AND the column, because the identifier columns this
    /// fires on appear under the same name in several tables — "`tenant_id` is
    /// not a `UUIDv7`" is not an actionable sentence without the table.
    #[error("{table}.{column} holds a value this build cannot read")]
    RowMalformed {
        /// Which table the column belongs to.
        table: &'static str,
        /// Which column refused.
        column: &'static str,
        /// The parse failure underneath.
        #[source]
        source: afd_core::error::Error,
    },

    /// An identifier this crate had to mint or read could not be formed.
    ///
    /// `#[from]`, so `?` lifts it — the identifier layer already says what was
    /// wrong with the value, and restating that here would add nothing and cost
    /// the `source()` chain.
    #[error(transparent)]
    Identifier {
        /// The identifier failure underneath.
        #[from]
        source: afd_core::error::Error,
    },

    /// The entropy source a ledger row's identifier is drawn from failed.
    ///
    /// `#[from]` for the same reason [`Error::Identifier`] is: the crypto layer
    /// already says what went wrong, and a charge that cannot mint a row id is
    /// a charge that did not land — which is what the caller needs, not a
    /// second sentence about randomness.
    #[error(transparent)]
    Entropy {
        /// The entropy failure underneath.
        #[from]
        source: afd_crypto::error::Error,
    },

    /// The pool would not give a connection.
    #[error("the billing store's datastore is unavailable")]
    Datastore {
        /// The pool failure underneath.
        #[from]
        source: afd_db::Error,
    },
}

impl Error {
    /// The sentence a caller is told.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self {
            Self::Query { .. }
            | Self::RowMalformed { .. }
            | Self::Identifier { .. }
            | Self::Entropy { .. } => DETAIL_OPERATION_FAILED,
            Self::Datastore { .. } => DETAIL_UNAVAILABLE,
        }
    }

    /// Whether the datastore behind this crate could not be reached.
    ///
    /// The question a gate's POSTURE turns on: this crate answers a value or a
    /// failure and never decides what to do about one, because fail-open and
    /// fail-closed belong beside the gate's name rather than beside the
    /// connection — the separation this crate is built around.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(self, Self::Datastore { .. })
    }

    /// The registry code a caller is refused with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Query { .. }
            | Self::RowMalformed { .. }
            | Self::Identifier { .. }
            | Self::Entropy { .. } => error_code::INTERNAL_OPERATION_FAILED,
            Self::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
        }
    }
}

/// The sentence a statement that would not run earns.
const DETAIL_OPERATION_FAILED: &str = "The billing operation could not be completed";

/// The sentence an unreachable datastore earns.
const DETAIL_UNAVAILABLE: &str = "Service temporarily unavailable";

/// This crate's result, defaulting to its own error.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::Query { context, source }
}

/// Reports a stored value this build cannot read, naming table and column.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| Error::RowMalformed {
        table,
        column,
        source,
    }
}
