//! What this crate refuses, and what it reports.
//!
//! The hull — the boxed struct, the backtrace, `Display`, `source()` — comes
//! from [`af_core::error_shell!`]. What is here is the part that varies: the
//! kinds this plane can fail with, the code and sentence each answers, and the
//! raisers that bind data.

use af_core::error_code::{self, ErrorCode};

#[cfg(test)]
mod tests;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// A policy or gate failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore backing the gate plane would not answer")]
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

    /// The caller sent something this plane will not accept.
    ///
    /// The only variant whose sentence is written FOR the caller; every other
    /// answers a fixed registry sentence and keeps its detail in the log.
    #[error("{detail}")]
    Rejected { detail: &'static str },

    /// A credential the policy assembly had to resolve could not be.
    #[error("the credential plane could not answer for the gate plane")]
    Credential {
        #[source]
        source: afd_credential::Error,
    },

    /// A fleet's approved GitHub scope would not parse back into a binding.
    #[error("the fleet's approved GitHub scope could not be read")]
    Scope {
        #[source]
        source: af_fleet_runtime::Error,
    },
}

impl Error {
    /// The registry code this failure answers with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Query { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::Rejected { .. } => error_code::INVALID_REQUEST,
            // A stored row this daemon wrote that will not read back is THIS
            // daemon failing, not the caller's request being wrong.
            ErrorKind::Scope { .. } => error_code::INTERNAL_OPERATION_FAILED,
            // Delegated rather than restated: the credential plane already
            // decides which of its own failures is an outage and which is a
            // fault, and a second copy of that mapping here is what drifts.
            ErrorKind::Credential { source } => source.code(),
        }
    }

    /// The sentence the caller is told.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self.kind() {
            ErrorKind::Rejected { detail } => detail,
            ErrorKind::Datastore { .. } => DETAIL_UNAVAILABLE,
            ErrorKind::Query { .. } | ErrorKind::Scope { .. } => DETAIL_OPERATION_FAILED,
            ErrorKind::Credential { source } => source.detail(),
        }
    }

    /// Whether the datastore or queue behind this crate could not be reached.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. } => true,
            ErrorKind::Credential { source } => source.is_datastore_unavailable(),
            _served @ (ErrorKind::Query { .. }
            | ErrorKind::Rejected { .. }
            | ErrorKind::Scope { .. }) => false,
        }
    }

    /// Whether the caller sent something this plane will not accept.
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self.kind(), ErrorKind::Rejected { .. })
    }

    /// Whether this failure is a stored-configuration fault a human must fix.
    ///
    /// Delegated to the credential plane where it owns the answer: a stored
    /// endpoint the guard refused does not become safe by being retried, while
    /// a datastore that would not answer recovers on its own. An approved
    /// scope that will not read back stays unreadable until a person asks for
    /// the access again.
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        match self.kind() {
            ErrorKind::Credential { source } => source.is_config_permanent(),
            ErrorKind::Scope { .. } => true,
            _transient @ (ErrorKind::Datastore { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Rejected { .. }) => false,
        }
    }
}

/// The sentence an unreachable datastore or queue earns.
use af_core::error::DETAIL_DATABASE_UNAVAILABLE as DETAIL_UNAVAILABLE;

/// The sentence a statement that would not run earns.
const DETAIL_OPERATION_FAILED: &str = "The operation could not be completed";

af_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_credential::Error => Credential,
    af_fleet_runtime::Error => Scope,
);

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Refuses a caller, naming why in their language.
pub(crate) fn rejected(detail: &'static str) -> Error {
    ErrorKind::Rejected { detail }.into()
}
