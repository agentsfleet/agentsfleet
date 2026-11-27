//! What sealing or checking a chunk refuses.
//!
//! A refusal reads, on the host's journal, as the checkpoint being
//! incompatible with its registration, the code a restore that cannot use a
//! checkpoint already logs; nothing here is a tenant's or a client's to read.

use af_core::error_code::{self, ErrorCode};

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

af_core::error_shell!(
    /// A chunk that could not be sealed or that failed its check.
    pub struct Error(ErrorKind);
);

/// Every way sealing or checking a chunk fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The bytes, or what the daemon handed over with them, are not the ones
    /// it registered.
    #[error("{detail}")]
    Refused {
        /// Which check failed.
        detail: &'static str,
    },

    /// The cipher refused the key, or the tag did not authenticate.
    #[error("the chunk could not be sealed or opened")]
    Crypto {
        /// The cipher's reason.
        #[source]
        source: af_crypto::error::Error,
    },

    /// The object's identifier is not one this system mints.
    #[error("the object identifier is not valid")]
    Identifier {
        /// The parser's reason.
        #[source]
        source: af_core::error::Error,
    },

    /// The key or the nonce is not base64.
    #[error("{detail}")]
    Encoding {
        /// Which value.
        detail: &'static str,
        /// The decoder's reason.
        #[source]
        source: base64::DecodeError,
    },
}

af_core::error_lifts!(Error, ErrorKind:
    af_crypto::error::Error => Crypto,
    af_core::error::Error => Identifier,
);

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Refused { .. }
            | ErrorKind::Crypto { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Encoding { .. } => error_code::RUN_CHECKPOINT_INCOMPATIBLE,
        }
    }
}

/// A check that failed: `detail` says which.
pub(crate) fn refused(detail: &'static str) -> Error {
    ErrorKind::Refused { detail }.into()
}

/// A base64 value that would not decode.
pub(crate) fn encoding(detail: &'static str, source: base64::DecodeError) -> Error {
    ErrorKind::Encoding { detail, source }.into()
}
