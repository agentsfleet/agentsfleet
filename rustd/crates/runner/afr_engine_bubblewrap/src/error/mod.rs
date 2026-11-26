//! What only the bubblewrap engine refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. The engine answers the seam's errors, into
//! which every building block's failure lifts; this type holds the one
//! failure no building block raises: a sandbox whose executor never
//! answered. It crosses the seam as itself. The code is the registry's
//! existing one, as every runner crate's is (`docs/RUST_ERROR_STANDARD.md`).

use std::time::Duration;

use af_core::error_code::{self, ErrorCode};

af_core::error_shell!(
    /// A bubblewrap failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails on its own.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The executor could not be reached within the ready timeout.
    #[error("the sandbox's executor did not answer within {waited:?}")]
    NotReady {
        /// How long the engine waited.
        waited: Duration,
        /// Why the last attempt failed: a socket not there yet, or a refusal
        /// waiting could not fix.
        #[source]
        source: afr_executor::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::NotReady { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

impl afr_engine::Failure for Error {
    /// None: a sandbox that never answered is the engine's own failure.
    fn seam(&self) -> Option<&afr_engine::Error> {
        None
    }
}

impl From<Error> for afr_engine::Error {
    fn from(error: Error) -> Self {
        Self::engine(error)
    }
}

/// Reports an executor not reached within `waited`, with why.
pub(crate) fn not_ready(waited: Duration) -> impl Fn(afr_executor::Error) -> afr_engine::Error {
    move |source| Error::from(ErrorKind::NotReady { waited, source }).into()
}
