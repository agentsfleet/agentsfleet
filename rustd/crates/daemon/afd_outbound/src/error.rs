//! The one error type this crate returns, and what each failure means for the
//! job that raised it.
//!
//! # Almost nothing here is an error, and that is the design
//!
//! A worker that returns `Err` up its loop stops delivering. So a failure to
//! POST, a bot token that is gone, an event row that vanished — none of them
//! are errors: they are [`crate::Verdict`]s, which the lane acts on by
//! stamping, releasing or abandoning the obligation. Only what stops the
//! worker from claiming or recording reaches this type: the host would not
//! draw the entropy a claim token is minted from, the ledger's pool would not
//! answer, or the ledger refused a claim, stamp, release or abandon.
//!
//! The two categories have different types, so a raise site cannot put a
//! delivery failure where a loop failure goes.

use af_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// An outbound-delivery failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The host could not draw the entropy a claim token is minted from.
    #[error("a claim token could not be drawn")]
    Entropy {
        #[source]
        source: af_crypto::error::Error,
    },

    /// The obligation ledger's pool would not hand out a connection.
    ///
    /// The ledger is the queue, so an unanswered pool stops claims until it
    /// answers again; nothing owed is lost meanwhile.
    #[error("the ledger holding the delivery obligations would not answer")]
    Ledger {
        #[source]
        source: afd_db::Error,
    },

    /// A statement reached PostgreSQL and was refused.
    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },
}

/// Reports a statement that reached PostgreSQL and was refused.
///
/// `map_err` that ADDS the one thing the call site alone knows — which
/// statement was running — and nothing else. The `sqlx::Error` rides through as
/// `#[source]` so the chain stays intact (`RUST_ERROR_STANDARD` rule 3).
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::from(ErrorKind::Query { context, source })
}

/// One sample of each kind a caller can match on, for a crate wrapping this
/// error to render in its own suite.
#[cfg(feature = "test-util")]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let (entropy, control) = af_crypto::entropy::Entropy::new_mocked();
    control.fail_next();
    let refused_draw = entropy
        .uuid_randomness()
        .expect_err("a mocked source told to fail refuses the draw");
    vec![
        (
            "ledger",
            ErrorKind::Ledger {
                source: afd_db::error::invalid_bool_knob("MIGRATE_ON_START"),
            }
            .into(),
        ),
        ("query", query("owe delivery")(sqlx::Error::RowNotFound)),
        // Listed last: a wrapping crate that takes the first kind that is not
        // an outage keeps reading the refused statement.
        ("entropy", refused_draw.into()),
    ]
}

impl Error {
    /// The registry code this failure answers with.
    ///
    /// Decided by [`Self::is_datastore_unavailable`], whose `match` names every
    /// variant: a new one must not inherit another's code by forgetting to
    /// extend anything, and that match stops compiling until it is listed.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        if self.is_datastore_unavailable() {
            error_code::INTERNAL_DB_UNAVAILABLE
        } else {
            error_code::INTERNAL_OPERATION_FAILED
        }
    }

    /// Whether the ledger could not be reached.
    ///
    /// The accessor `afd_admission` and its siblings carry, so a crate
    /// wrapping this error asks the question instead of comparing codes.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            // An unreachable store is the outage an operator retries against,
            // and a pool that will not hand out a connection is the same
            // outage. Anything either store ANSWERS is this daemon's own fault.
            ErrorKind::Ledger { .. } => true,
            ErrorKind::Entropy { .. } | ErrorKind::Query { .. } => false,
        }
    }
}

af_core::error_lifts!(Error, ErrorKind:
    af_crypto::error::Error => Entropy,
    afd_db::Error => Ledger,
);
