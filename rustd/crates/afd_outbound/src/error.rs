//! The one error type this crate returns, and what each failure means for the
//! job that raised it.
//!
//! # Almost nothing here is an error, and that is the design
//!
//! A worker that returns `Err` up its loop stops delivering. So a failure to
//! POST, a bot token that is gone, an event row that vanished — none of them
//! are errors: they are [`crate::Verdict`]s, which the worker acts on and then
//! acknowledges the job. Only the things that make the LOOP unable to continue
//! reach this type: the stream would not answer, the group could not be
//! created, the dedicated connection would not open.
//!
//! `worker.zig` reaches the same split by returning `Outcome` from every
//! delivery path and swallowing its Dragonfly errors into a `catch` that logs. The
//! difference is that here the two categories have different types, so a raise
//! site cannot put a delivery failure where a loop failure goes.

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// An outbound-delivery failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The queue would not answer, or the connection to it would not open.
    ///
    /// The only failure that stops the worker's loop rather than one job: a
    /// stream that cannot be read has no next job to move on to.
    #[error("the queue holding the outbound answers would not answer")]
    Queue {
        #[source]
        source: afd_dragonfly::Error,
    },

    /// The obligation ledger's pool would not hand out a connection.
    ///
    /// Distinct from [`Self::Queue`] because the two stores fail for different
    /// reasons and an operator acts on them differently: a queue outage stops
    /// the worker's loop, while a ledger that will not answer leaves the
    /// delivery itself unaffected and only the RECORD of it unwritten.
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
///
/// The queue kind is left out: it wraps `afd_dragonfly`'s error, and a wrapper
/// that needs a queue sample takes it from that crate's own builder.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    vec![
        (
            "ledger",
            ErrorKind::Ledger {
                source: afd_db::error::invalid_bool_knob("MIGRATE_ON_START"),
            }
            .into(),
        ),
        ("query", query("owe delivery")(sqlx::Error::RowNotFound)),
    ]
}

impl Error {
    /// The registry code this failure answers with.
    ///
    /// A `match` over named variants rather than a bare expression, because a
    /// new variant must not be able to inherit another's code by forgetting to
    /// extend anything — the match stops compiling until it is listed.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            // An unreachable store is the outage an operator retries against,
            // and a pool that will not hand out a connection is the same
            // outage. Anything either store ANSWERS is this daemon's own fault.
            //
            // Sharing one arm rather than repeating the constant, because
            // clippy reads two arms with one body as a copy-paste. Every
            // variant is still named, which is what the note above is actually
            // asking for: a third one cannot inherit a code by being forgotten,
            // because the match stops compiling until it is listed.
            ErrorKind::Queue { source } if source.is_unavailable() => {
                error_code::INTERNAL_DB_UNAVAILABLE
            }
            ErrorKind::Ledger { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Queue { .. } | ErrorKind::Query { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
        }
    }
}

afd_core::error_lifts!(Error, ErrorKind:
    afd_dragonfly::Error => Queue,
    afd_db::Error => Ledger,
);
