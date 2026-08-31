//! The one error type this crate returns, and the wire code each failure maps to.
//!
//! Shaped per `M-ERRORS-CANONICAL-STRUCTS`: a struct carrying a captured
//! backtrace and a private kind, with `is_*` accessors instead of a public
//! enum. Callers get to ask the questions they can act on — "was this a
//! malformed identifier?" — without the crate promising never to grow a new
//! internal failure mode.
//!
//! A malformed error CODE is not among the failures here: `ErrorCode::declare`
//! checks its grammar during constant evaluation, so a bad spelling never
//! reaches runtime and needs no variant.
//!
//! Every error also answers [`Error::code`], the `UZ-*` code a handler would
//! put on the wire. Keeping that mapping beside the error rather than in each
//! handler is what stops two call sites from reporting the same failure under
//! two different codes.

use crate::error_code::{self, ErrorCode};

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

crate::error_shell!(
    /// A domain-primitive failure: a value did not satisfy the invariant its
    /// type encodes.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Private so a new variant is not a breaking change.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("identifier is not a canonical lowercase dashed version-7 UUID: {reason}")]
    IdShape { reason: &'static str },

    #[error("{name} must be within {min}..={max}, got {value}")]
    OutOfRange {
        name: &'static str,
        value: u32,
        min: u32,
        max: u32,
    },
}

impl Error {
    /// Whether an identifier failed the canonical version-7 UUID shape.
    #[must_use]
    pub fn is_id_shape(&self) -> bool {
        matches!(*self.kind(), ErrorKind::IdShape { .. })
    }

    /// Whether a bounded value fell outside the range its type permits.
    #[must_use]
    pub fn is_out_of_range(&self) -> bool {
        matches!(*self.kind(), ErrorKind::OutOfRange { .. })
    }

    /// The registry code a handler would surface for this failure.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match *self.kind() {
            ErrorKind::IdShape { .. } => error_code::UUIDV7_INVALID_ID_SHAPE,
            ErrorKind::OutOfRange { .. } => error_code::INVALID_REQUEST,
        }
    }
}

/// The sentence a caller is told when a datastore cannot be reached.
///
/// `problem_response.zig`'s `internalDbUnavailable`, byte for byte. Declared
/// here because every plane answers it and three of them had drifted onto an
/// invented "Service temporarily unavailable" — which meant one condition
/// reaching a client as two different sentences depending on which plane the
/// request happened to hit.
pub const DETAIL_DATABASE_UNAVAILABLE: &str = "Database unavailable";
