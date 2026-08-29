//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_ingress::Error`] and `afd_db::Error`
//! (`M-ERRORS-CANONICAL-STRUCTS`): a struct carrying a captured backtrace over
//! a private kind, with the code and the sentence decided together in one table
//! rather than spelled at each raise site.
//!
//! # A vendor being down is not the same failure as a vendor saying no
//!
//! [`ErrorKind::UpstreamUnreachable`] and [`ErrorKind::UpstreamRefused`] are
//! kept apart because a caller does different things with them (RULE ECL). The
//! first is retryable and `:sync` will repair it on its own; the second is a
//! request this daemon composed that the external scheduler will refuse again
//! however many times it is sent, and retrying it forever would turn one bad
//! cron expression into a permanent outbound load. The Zig collapses both onto
//! one `error.QStashRequestFailed`, and that is the delta this port fixes
//! rather than carries (RULE PORT).
//!
//! # Nothing an OPERATOR did wrong is an error here
//!
//! A cron expression this daemon will not accept, a fleet already holding its
//! full complement of schedules, a schedule another syncer is holding — none of
//! those reach this type. They are refusals the caller renders, because nothing
//! failed: the store did its job and answered no.

use afd_core::error_code::{self, ErrorCode};

pub mod detail;
mod raise;

pub(crate) use self::raise::{query, row_unreadable, upstream_refused, upstream_unreadable};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A schedule failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore holding the schedule row would not answer")]
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

    /// The identifier source would not answer, or produced no usable value.
    ///
    /// One kind for both halves of a mint, because a caller does the same thing
    /// with either: a schedule cannot be created without an identifier, and
    /// neither failure is one an operator can act on differently.
    #[error("a schedule identifier could not be minted")]
    Identifier {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("a minted schedule identifier is not a canonical one")]
    IdentifierShape {
        #[source]
        source: afd_core::error::Error,
    },

    #[error("the queue would not take the fire")]
    Queue {
        #[source]
        source: afd_redis::Error,
    },

    /// The external scheduler could not be reached at all.
    ///
    /// A transport failure, a timeout, a DNS answer that never came. Retryable
    /// by construction: nothing upstream has seen the request, so sending it
    /// again cannot double anything.
    #[error("the external scheduler could not be reached")]
    UpstreamUnreachable {
        #[source]
        source: reqwest::Error,
    },

    /// The external scheduler answered, and the answer was no.
    ///
    /// Carries the status because it is the one fact that decides what an
    /// operator does next — a 401 is a credential to rotate, a 4xx is a request
    /// to fix, a 5xx is a vendor incident to wait out.
    #[error("the external scheduler refused the request with status {status}")]
    UpstreamRefused {
        /// The HTTP status, as the vendor sent it.
        status: u16,
    },

    /// A column of the schedule row this build cannot make sense of.
    ///
    /// A `desired_status` spelled by a newer daemon, or an id that is not a
    /// canonical identifier. Neither is defaulted past — see [`crate::model`]
    /// on what defaulting one would cost.
    #[error("the schedule row's {column} is not a value this build can read")]
    RowUnreadable {
        /// The column, so an operator knows which one to go and look at.
        column: &'static str,
    },
}

/// The columns [`ErrorKind::RowUnreadable`] can name, one spelling each.
pub(crate) const COLUMN_ID: &str = "id";
/// See [`COLUMN_ID`].
pub(crate) const COLUMN_FLEET: &str = "fleet_id";
/// See [`COLUMN_ID`].
pub(crate) const COLUMN_SOURCE: &str = "source";
/// See [`COLUMN_ID`].
pub(crate) const COLUMN_DESIRED_STATUS: &str = "desired_status";
/// See [`COLUMN_ID`].
pub(crate) const COLUMN_SYNC_STATUS: &str = "sync_status";
/// See [`COLUMN_ID`].
pub(crate) const COLUMN_WORKSPACE: &str = "workspace_id";

impl Error {
    /// Whether sending the same request again could succeed.
    ///
    /// The question `:sync` asks, and the reason the two upstream variants are
    /// separate — see the module note. A datastore that would not answer is
    /// retryable for the same reason a vendor that would not answer is.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::UpstreamUnreachable { .. } | ErrorKind::Datastore { .. }
        )
    }

    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            // A queue that is GONE is the same outage a caller retries against,
            // so it answers the unavailable code rather than a generic 500.
            ErrorKind::Queue { source } if source.is_unavailable() => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            ErrorKind::Query { .. } | ErrorKind::RowUnreadable { .. } => {
                (error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR)
            }
            // The vendor's own status is NOT surfaced to the caller. A person
            // editing a schedule cannot act on "QStash answered 429", and the
            // status is already in the row's `last_error` where an operator
            // reads it. What they are told is that the schedule is not yet
            // registered and that `:sync` will retry.
            ErrorKind::UpstreamUnreachable { .. } | ErrorKind::UpstreamRefused { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::UPSTREAM_UNAVAILABLE,
            ),
            ErrorKind::Queue { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::IdentifierShape { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::OPERATION_FAILED,
            ),
        }
    }

    /// The registry code this failure answers with.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.answer().0
    }

    /// The sentence the caller is told.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }
}
