//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_vault::Error`] and `afd_db::Error`
//! (`M-ERRORS-CANONICAL-STRUCTS`, with the workspace's declared divergence): a
//! struct carrying a captured backtrace over a private kind, with the code and
//! the sentence decided together in one table rather than spelled at each
//! `fail` site.
//!
//! # Nothing a SENDER did wrong is an error here
//!
//! A delivery with no signature, a stale timestamp, a fleet with no configured
//! secret — none of those reach this type. They are `afd_webhook::Verdict`, or
//! an `Ok(None)`, because nothing failed: the wall did its job and the ingress
//! answers a refusal. What lives here is the other half — THIS side being
//! broken. Keeping them apart is what stops an operator's alert on `Error`
//! from firing every time an internet scanner probes `/v1/webhooks/{id}`
//! (RULE ECL), and it is the same split `afd_webhook::verdict` draws one layer
//! up.
//!
//! The one apparent exception proves it: [`ErrorKind::ConfigUnreadable`] is a
//! document THIS daemon stored and can no longer parse, which is an operator's
//! incident and not a sender's.

use afd_core::error_code::{self, ErrorCode};

pub mod detail;
mod raise;

pub(crate) use self::raise::{query, row_unreadable};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// An ingress failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore holding the fleet row would not answer")]
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

    #[error("the vault would not open this fleet's signing secret")]
    Vault {
        #[source]
        source: afd_vault::Error,
    },

    #[error("the queue would not take the delivery")]
    Queue {
        #[source]
        source: afd_redis::Error,
    },

    #[error("the stored fleet document no longer parses")]
    ConfigUnreadable {
        #[source]
        source: afd_fleet_runtime::Error,
    },

    /// A column of the fleet row this build cannot make sense of.
    ///
    /// A status spelled by a newer daemon, or a `workspace_id` that is not a
    /// canonical identifier. Neither is defaulted past: the status because
    /// reading a state this build does not understand as `installing` would let
    /// it act on the wrong one, and the workspace because the column is a
    /// NOT NULL foreign key, so a value that will not parse is a broken
    /// invariant rather than a race.
    #[error("the fleet row's {column} is not a value this build can read")]
    RowUnreadable {
        /// The column, so an operator knows which one to go and look at.
        column: &'static str,
    },
}

/// The columns [`ErrorKind::RowUnreadable`] can name, one spelling each.
pub(crate) const COLUMN_STATUS: &str = "status";
/// See [`COLUMN_STATUS`].
pub(crate) const COLUMN_WORKSPACE: &str = "workspace_id";
/// See [`COLUMN_STATUS`].
pub(crate) const COLUMN_FLEET: &str = "id";

impl Error {
    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            // A queue that is GONE is the same outage a caller retries against,
            // so it answers the unavailable code rather than a generic 500 —
            // the distinction the dashboard's retry turns on. A queue that
            // answered and refused is this process's problem.
            ErrorKind::Queue { source } if source.is_unavailable() => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            ErrorKind::Query { .. } | ErrorKind::RowUnreadable { .. } => {
                (error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR)
            }
            // Three internal failures, one fixed sentence. Naming which of them
            // it was would tell whoever provoked it something about this
            // deployment's stored state, and a webhook sender is exactly the
            // caller who must not learn it.
            ErrorKind::Vault { .. }
            | ErrorKind::Queue { .. }
            | ErrorKind::ConfigUnreadable { .. } => (
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

    /// Whether a datastore behind this crate could not be reached.
    ///
    /// The question the HTTP edge turns on: an outage is this instance's to
    /// report as a 503, where every other failure here is a 500 (RULE ECL).
    /// Both stores count — a fleet row this daemon cannot read and a stream it
    /// cannot append to are the same incident to a sender that will retry.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. } => true,
            ErrorKind::Queue { source } => source.is_unavailable(),
            _reachable => false,
        }
    }
}
