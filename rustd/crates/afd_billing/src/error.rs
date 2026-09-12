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
    billing_wallet_missing, charges_cursor_invalid, query, row_malformed,
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

    /// An identifier this crate had to mint or read could not be formed.
    ///
    /// Transparent, and lifted by `error_lifts!`: the identifier layer already
    /// says what was wrong with the value, and restating it here would add
    /// nothing and cost the `source()` chain.
    #[error(transparent)]
    Identifier { source: afd_core::error::Error },

    /// The entropy source a ledger row's identifier is drawn from failed.
    ///
    /// Transparent for the same reason [`ErrorKind::Identifier`] is: a charge
    /// that cannot mint a row id is a charge that did not land, which is what
    /// the caller needs rather than a second sentence about randomness.
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
            | ErrorKind::Identifier { .. }
            | ErrorKind::Entropy { .. } => (
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
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Datastore { .. })
    }
}

/// The sentence a statement that would not run earns.
const DETAIL_OPERATION_FAILED: &str = "The billing operation could not be completed";

/// The sentence a tenant with no wallet row earns.
///
/// The em-dash sentence is `tenant_billing.zig`'s, byte for byte: the row is
/// written in the tenant-create transaction, so its absence is a bootstrap
/// invariant broken by surgery or a defect, and the sentence says whose problem
/// that is. Carried across from `afd_tenant` unchanged when the reader moved.
const DETAIL_WALLET_MISSING: &str = "Tenant billing row missing — bootstrap invariant violated";

/// The refusal for a charges cursor this daemon never issued.
///
/// Lower-case and terse where the keyset cursor's refusals are sentences,
/// because this is `tenant_billing.zig`'s exact spelling and a cursor may be
/// judged by either binary mid-cutover. Re-authoring it here would have made
/// the two daemons answer differently for one condition.
const DETAIL_CURSOR_INVALID: &str = "invalid cursor";

/// The sentence an unreachable datastore earns.
use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DETAIL_UNAVAILABLE;

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
