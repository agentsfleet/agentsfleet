//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_ingress::Error`] and `afd_db::Error`
//! (`M-ERRORS-CANONICAL-STRUCTS`, with the workspace's declared divergence): a
//! struct carrying a captured backtrace over a private kind, with the code and
//! the sentence decided together in one table rather than spelled at each
//! raise site. The hull is `af_core::error_shell!`, so nothing here repeats
//! what fifteen sibling crates already share.
//!
//! A failed durable write is returned to the producer. Notification failure
//! happens after acceptance and never changes that answer.

use af_core::error::{DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE, DETAIL_OPERATION_FAILED};
use af_core::error_code::{self, ErrorCode};

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

af_core::error_shell!(
    /// An admission failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A setup written under another build's semantics needs explicit review.
    #[error("the selected setup requires policy review")]
    Policy,

    /// No fleet with this id belongs to the caller's workspace.
    #[error("no such fleet in this workspace")]
    FleetAbsent,

    /// The fleet exists but its status takes no new work.
    #[error("the fleet is {} and takes no new work", status.as_str())]
    NotRunnable {
        status: afd_fleet_lifecycle::FleetStatus,
    },
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

    /// The deployment ceiling is spent, and the producer is told to come back later.
    #[error("the deployment admission budget of {limit} is spent")]
    OverBudget { limit: u64 },

    /// Postgres refused for want of a resource: disk, memory, connections.
    ///
    /// SQLSTATE class 53, told apart from every other statement failure
    /// because the cure is capacity and the caller should back off.
    #[error("statement refused for want of a resource during {context}")]
    Exhausted {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// Postgres gave up waiting: a lock held past `lock_timeout` (`55P03`) or
    /// a statement past `statement_timeout` (`57014`), the login's bounds
    /// (`afd_db::login`). Retryable: nothing was committed.
    #[error("statement timed out during {context}")]
    TimedOut {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// A row identifier could not be minted.
    #[error("a row identifier could not be minted")]
    Entropy {
        #[source]
        source: af_crypto::error::Error,
    },
}

impl Error {
    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Policy => (
                error_code::AUTHORING_POLICY,
                af_core::problem::Problem::of(error_code::AUTHORING_POLICY).title(),
            ),
            ErrorKind::FleetAbsent => (
                error_code::AGENTSFLEET_NOT_FOUND,
                af_core::problem::Problem::of(error_code::AGENTSFLEET_NOT_FOUND).title(),
            ),
            ErrorKind::NotRunnable { .. } => (
                error_code::AGENTSFLEET_PAUSED_INGRESS,
                af_core::problem::Problem::of(error_code::AGENTSFLEET_PAUSED_INGRESS).title(),
            ),
            ErrorKind::Datastore { .. }
            | ErrorKind::OverBudget { .. }
            | ErrorKind::Exhausted { .. }
            | ErrorKind::TimedOut { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                DETAIL_DATABASE_UNAVAILABLE,
            ),
            ErrorKind::Query { .. } => (error_code::INTERNAL_DB_QUERY, DETAIL_DATABASE_ERROR),
            ErrorKind::Entropy { .. } => (
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

    /// The sentence the caller is told — one of `af_core::error`'s three,
    /// never a spelling of this crate's own.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }

    /// Whether a datastore behind this crate could not be reached — or
    /// could not take more, which the caller retries the same way.
    ///
    /// The question the HTTP edge turns on: an outage is this instance's to
    /// report as a 503, where every other failure here is a 500 (RULE ECL).
    /// Capacity answers yes here because the producer's move is the same;
    /// [`Self::is_over_capacity`] is how the two are told apart.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. }
            | ErrorKind::OverBudget { .. }
            | ErrorKind::Exhausted { .. }
            | ErrorKind::TimedOut { .. } => true,
            ErrorKind::Policy
            | ErrorKind::FleetAbsent
            | ErrorKind::NotRunnable { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::Entropy { .. } => false,
        }
    }

    /// Whether no fleet with the id belongs to the caller's workspace.
    #[must_use]
    pub const fn is_fleet_absent(&self) -> bool {
        matches!(self.kind(), ErrorKind::FleetAbsent)
    }

    /// The status of a fleet that takes no new work, when that is the refusal.
    #[must_use]
    pub const fn not_runnable(&self) -> Option<afd_fleet_lifecycle::FleetStatus> {
        match self.kind() {
            ErrorKind::NotRunnable { status } => Some(*status),
            ErrorKind::Policy
            | ErrorKind::FleetAbsent
            | ErrorKind::Datastore { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::OverBudget { .. }
            | ErrorKind::Exhausted { .. }
            | ErrorKind::TimedOut { .. }
            | ErrorKind::Entropy { .. } => None,
        }
    }

    /// Whether the refusal was capacity: a spent budget, a queue that is
    /// full, or a Postgres out of a resource.
    #[must_use]
    pub const fn is_over_capacity(&self) -> bool {
        match self.kind() {
            ErrorKind::OverBudget { .. } | ErrorKind::Exhausted { .. } => true,
            ErrorKind::Policy
            | ErrorKind::FleetAbsent
            | ErrorKind::NotRunnable { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::TimedOut { .. }
            | ErrorKind::Entropy { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests;
