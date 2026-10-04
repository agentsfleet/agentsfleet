//! Why a scrub could not be built.
//!
//! The one failure composes the matcher's refusal as its source, so the error
//! takes the `afd_core::error_shell!` hull.

use afd_core::error_code::{self, ErrorCode};

afd_core::error_shell!(
    /// A scrub that could not be built, with the backtrace of where.
    pub struct Error(ErrorKind);
);

/// Every way building a scrub fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The matcher could not be built over the secret values.
    #[error("the secret scrub could not be built")]
    Matcher {
        /// The matcher's refusal.
        #[from]
        source: aho_corasick::BuildError,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Matcher { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

afd_core::error_lifts!(Error, ErrorKind:
    aho_corasick::BuildError => Matcher,
);
