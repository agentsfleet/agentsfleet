//! Why the egress client could not be built.
//!
//! The crate's one boot failure: the TLS backend or the client refused its
//! configuration. A refused or failed request is a [`Refusal`], which the
//! model reads; this is what the runner logs before it serves anything.
//!
//! [`Refusal`]: crate::Refusal

use afd_core::error_code::{self, ErrorCode};

afd_core::error_shell!(
    /// An egress client that could not be built, with the backtrace of where.
    pub struct Error(ErrorKind);
);

/// Every way building the client fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// reqwest refused the client's configuration.
    #[error("the egress client could not be built")]
    Client {
        /// reqwest's refusal.
        #[from]
        source: reqwest::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Client { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

afd_core::error_lifts!(Error, ErrorKind:
    reqwest::Error => Client,
);
