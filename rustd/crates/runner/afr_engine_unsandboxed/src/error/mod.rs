//! What building the debug engine refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. The engine itself answers the seam's errors;
//! this type is only what refuses to build it: a release build, which never
//! runs a tool call unsandboxed. The code is the registry's existing one, as
//! every runner crate's is (`docs/RUST_ERROR_STANDARD.md`).

use af_core::error_code::{self, ErrorCode};

af_core::error_shell!(
    /// A refusal to build the debug engine, with the backtrace of where it
    /// was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The unsandboxed engine was asked for in a release build.
    #[error("a release build never runs a tool call unsandboxed")]
    UnsandboxedInRelease,
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::UnsandboxedInRelease => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The failure in its own sentence, without its code or backtrace.
    #[must_use]
    pub fn detail(&self) -> String {
        self.kind().to_string()
    }
}
