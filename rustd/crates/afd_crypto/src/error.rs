//! The one error type this crate returns, and the wire code each failure maps to.
//!
//! Shaped per `M-ERRORS-CANONICAL-STRUCTS`: a struct carrying a captured
//! backtrace and a private kind, with `is_*` accessors rather than a public
//! enum, so a new internal failure mode is not a breaking change.
//!
//! # Why open failures do not say why
//!
//! [`Error::is_open_failed`] answers one question — the envelope did not open —
//! and deliberately cannot distinguish a wrong key from a tampered tag from a
//! mismatched associated data. Telling those apart is a decryption oracle. The
//! Zig daemon collapses them into `DecryptFailed` for the same reason, and both
//! report `UZ-INTERNAL-003` on the wire.

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to that crate's own [`Error`] — the shape
/// `core_api` has run in production on for years, and the one bun uses
/// (`pub type Result<T, E = Error>`). The default parameter is what lets the
/// few functions answering with a different error keep the same spelling:
/// `Result<T>` for the common case, `Result<T, OtherError>` where it differs.
///
/// The point is not brevity. It is that a reader never has to check WHICH
/// error a signature returns to know it is this crate's, and a new call site
/// cannot quietly introduce a second error type without saying so.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A cryptographic operation failed, or a value handed to one was malformed.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Private so a new variant is not a breaking change.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("master key must be {expected} hexadecimal characters, got {actual}")]
    KeyHexLength { expected: usize, actual: usize },

    #[error("master key is not valid hexadecimal")]
    KeyHexDigit,

    #[error("{component} must be exactly {expected} bytes, got {actual}")]
    ComponentLength {
        component: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("stored envelope declares KEK version {found}, only {supported} is supported")]
    UnsupportedVersion { found: i32, supported: i32 },

    #[error("envelope did not open")]
    OpenFailed,

    #[error("message authentication code did not verify")]
    MacMismatch,

    #[error("the system entropy source failed")]
    Entropy,
}

impl Error {
    /// Whether the configured master key was not 64 hexadecimal characters.
    ///
    /// Either case decodes — operators paste the key from both — so this asks
    /// about length and alphabet, not about capitalisation.
    #[must_use]
    pub fn is_key_hex(&self) -> bool {
        matches!(
            *self.kind(),
            ErrorKind::KeyHexLength { .. } | ErrorKind::KeyHexDigit
        )
    }

    /// Whether a stored envelope component had the wrong length or version.
    #[must_use]
    pub fn is_malformed_envelope(&self) -> bool {
        matches!(
            *self.kind(),
            ErrorKind::ComponentLength { .. } | ErrorKind::UnsupportedVersion { .. }
        )
    }

    /// Whether an envelope failed to open.
    ///
    /// Deliberately one question. See the module documentation: separating a
    /// wrong key from a tampered tag would be a decryption oracle.
    #[must_use]
    pub fn is_open_failed(&self) -> bool {
        matches!(*self.kind(), ErrorKind::OpenFailed)
    }

    /// Whether a message authentication code did not verify.
    #[must_use]
    pub fn is_mac_mismatch(&self) -> bool {
        matches!(*self.kind(), ErrorKind::MacMismatch)
    }

    /// Whether the operating system's entropy source refused to produce bytes.
    #[must_use]
    pub fn is_entropy(&self) -> bool {
        matches!(*self.kind(), ErrorKind::Entropy)
    }

    /// The registry code a handler would surface for this failure.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match *self.kind() {
            ErrorKind::ComponentLength { .. } | ErrorKind::UnsupportedVersion { .. } => {
                error_code::VAULT_DATA_INVALID
            }
            _ => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}
