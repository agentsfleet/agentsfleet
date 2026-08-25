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

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

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

/// A domain-primitive failure: a value did not satisfy the invariant its type encodes.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    backtrace: Backtrace,
}

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
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            backtrace: Backtrace::capture(),
        }
    }

    /// Whether an identifier failed the canonical version-7 UUID shape.
    #[must_use]
    pub fn is_id_shape(&self) -> bool {
        matches!(self.kind, ErrorKind::IdShape { .. })
    }

    /// Whether a bounded value fell outside the range its type permits.
    #[must_use]
    pub fn is_out_of_range(&self) -> bool {
        matches!(self.kind, ErrorKind::OutOfRange { .. })
    }

    /// The registry code a handler would surface for this failure.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind {
            ErrorKind::IdShape { .. } => error_code::UUIDV7_INVALID_ID_SHAPE,
            ErrorKind::OutOfRange { .. } => error_code::INVALID_REQUEST,
        }
    }

    /// The backtrace captured when this error was constructed.
    ///
    /// Empty unless `RUST_BACKTRACE` asked for one — capturing is opt-in, so
    /// the common path costs a few instructions rather than microseconds.
    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code().as_str(), self.kind)?;
        if self.backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            write!(f, "\n{}", self.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    /// The failure beneath this one, skipping our own kind.
    ///
    /// Returning `&self.kind` here — which every crate in this workspace did —
    /// makes an error repeat itself: `Display` already renders the kind's
    /// message, so a chain walker prints it twice before reaching anything new.
    /// The kind is not a CAUSE of this error, it IS this error; what caused it
    /// is whatever the kind wraps.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.kind)
    }
}
