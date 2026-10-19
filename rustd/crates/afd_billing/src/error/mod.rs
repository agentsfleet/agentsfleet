//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_events::Error`] and `afd_admission::Error`: a struct
//! carrying a captured backtrace over a private kind, with the code and the
//! sentence decided together in one table rather than spelled at each raise
//! site. The hull is `afd_core::error_shell!`, so nothing here repeats what its
//! sibling crates already share, and the boxed kind keeps `Result` pointer-sized
//! on the `Ok` path — the shape every statement in this crate returns through.
//!
//! # A cursor this daemon did not mint is not a failure of this daemon
//!
//! [`ErrorKind::ChargesCursorInvalid`] and [`ErrorKind::WalletMissing`] carry no
//! source: nothing failed underneath either one. The bytes were simply not a
//! cursor, and a tenant with no wallet row is an invariant that was already
//! broken before this crate looked (`RUST_ERROR_STANDARD` rule 4's second half).

use afd_core::error_code::{self, ErrorCode};

mod raise;

#[cfg(feature = "test-util")]
pub use self::raise::one_of_each_kind;
pub(crate) use self::raise::{
    billing_wallet_missing, charge_overflow, charges_cursor_invalid, query, row_malformed,
};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1). Hand-written on purpose: an alias that only
/// appeared after macro expansion is one a reader cannot see.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A billing failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A statement would not run.
    #[error("the billing store could not {context}")]
    Query {
        context: &'static str,
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
        table: &'static str,
        column: &'static str,
        #[source]
        source: afd_core::error::Error,
    },

    /// A ledger row's identifier could not be minted.
    ///
    /// Transparent: a charge that cannot mint a row id is a charge that did not
    /// land, which is what the caller needs rather than a second sentence about
    /// randomness.
    #[error(transparent)]
    Entropy { source: afd_crypto::error::Error },

    /// A tenant reached billing with no wallet row behind it.
    ///
    /// A broken invariant rather than a race: every tenant is given a wallet at
    /// signup, so a missing one is a row that should exist and does not.
    #[error("a tenant reached billing with no wallet row behind it")]
    WalletMissing,

    /// A charges cursor this daemon never issued.
    #[error("a charges cursor this daemon never issued")]
    ChargesCursorInvalid,

    /// A priced line or a balance would not fit in a signed 64-bit nano count.
    ///
    /// No source: nothing failed underneath; the arithmetic refused a value
    /// rather than wrap it into a wrong charge.
    #[error("a charge does not fit in a 64-bit nano count")]
    ChargeOverflow,

    /// The pool would not give a connection.
    #[error("the billing store's datastore is unavailable")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },
}

impl Error {
    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            // The caller's to correct, unlike everything else here.
            ErrorKind::ChargesCursorInvalid => (error_code::INVALID_REQUEST, DETAIL_CURSOR_INVALID),
            ErrorKind::WalletMissing => {
                (error_code::INTERNAL_OPERATION_FAILED, DETAIL_WALLET_MISSING)
            }
            ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::ChargeOverflow => (
                error_code::INTERNAL_OPERATION_FAILED,
                DETAIL_OPERATION_FAILED,
            ),
            ErrorKind::Datastore { .. } => {
                (error_code::INTERNAL_DB_UNAVAILABLE, DETAIL_UNAVAILABLE)
            }
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
    /// the log, and a caller who could read it would learn which statement this
    /// daemon runs.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }

    /// Whether the datastore behind this crate could not be reached.
    ///
    /// The question a gate's POSTURE turns on: this crate answers a value or a
    /// failure and never decides what to do about one, because fail-open and
    /// fail-closed belong beside the gate's name rather than beside the
    /// connection — the separation this crate is built around.
    ///
    /// Two shapes of one outage: the pool could not hand out a connection, or a
    /// statement's connection was lost mid-flight (a failover, a reset socket, a
    /// shutdown answer) as `afd_db::error::is_connection_lost` decides. A
    /// statement that failed on its own terms (a constraint, a syntax or decode
    /// error) is a defect, not an outage.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. } => true,
            ErrorKind::Query { source, .. } => afd_db::error::is_connection_lost(source),
            _ => false,
        }
    }

    /// Whether a statement gave up waiting: on a row lock another transaction
    /// held past the lock timeout (SQLSTATE `55P03`), or past the statement
    /// timeout (`57014`).
    ///
    /// Contention, not an outage: the datastore answered, and it answered
    /// "not now". It still leaves the biller without an answer, so
    /// [`Self::is_unanswered`] counts it with an outage, and the outage policy
    /// decides what the work does.
    #[must_use]
    pub fn is_contended(&self) -> bool {
        let ErrorKind::Query { source, .. } = self.kind() else {
            return false;
        };
        source
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .is_some_and(|code| {
                code == SQLSTATE_LOCK_NOT_AVAILABLE || code == SQLSTATE_QUERY_CANCELED
            })
    }

    /// Whether the biller could not answer: an outage or contention. What the
    /// outage policy turns on.
    #[must_use]
    pub fn is_unanswered(&self) -> bool {
        self.is_datastore_unavailable() || self.is_contended()
    }

    /// Whether the payer has no wallet row at all.
    ///
    /// A gate admits such a tenant rather than refusing it: the row is written
    /// at tenant creation, so its absence is an operator gap, and refusing every
    /// run for it would turn a provisioning defect into an outage.
    #[must_use]
    pub fn is_wallet_missing(&self) -> bool {
        matches!(self.kind(), ErrorKind::WalletMissing)
    }
}

/// A row lock was not granted within the lock timeout.
const SQLSTATE_LOCK_NOT_AVAILABLE: &str = "55P03";

/// The statement ran past its timeout and was cancelled.
const SQLSTATE_QUERY_CANCELED: &str = "57014";

/// The sentence a statement that would not run earns.
const DETAIL_OPERATION_FAILED: &str = "The billing operation could not be completed";

/// The sentence a tenant with no wallet row earns.
///
/// The row is written in the tenant-create transaction, so its absence is a
/// bootstrap invariant broken by surgery or a defect, and the sentence says
/// whose problem that is.
const DETAIL_WALLET_MISSING: &str = "Tenant billing row missing — bootstrap invariant violated";

/// The refusal for a charges cursor this daemon never issued.
///
/// Lower-case and terse where the keyset cursor's refusals are sentences: a
/// cursor is this daemon's own receipt handed back, and the refusal describes
/// nothing about its format to whoever is probing it.
const DETAIL_CURSOR_INVALID: &str = "invalid cursor";

/// The sentence an unreachable datastore earns.
use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DETAIL_UNAVAILABLE;

#[cfg(test)]
mod tests;
