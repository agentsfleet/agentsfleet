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
//! delivery path and swallowing its Redis errors into a `catch` that logs. The
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
        source: afd_redis::Error,
    },
}

impl Error {
    /// The registry code this failure answers with.
    ///
    /// One variant, so one arm — and a `match` rather than a bare expression
    /// because a second variant must not be able to inherit this one's code by
    /// forgetting to extend anything.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            // An unreachable queue is the outage an operator retries against;
            // anything else it answers is this daemon's own fault.
            ErrorKind::Queue { source } if source.is_unavailable() => {
                error_code::INTERNAL_DB_UNAVAILABLE
            }
            ErrorKind::Queue { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

afd_core::error_lifts!(Error, ErrorKind:
    afd_redis::Error => Queue,
);
