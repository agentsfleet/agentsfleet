//! An engine's or a building block's own failure, as the seam carries it.
//!
//! An engine raises failures the seam has no kind for: a launcher that exited,
//! a kernel that refused a netlink step, a toolbox image that failed its
//! digest. Each crate keeps its own error type for those, and the seam carries
//! it whole: its registry code, its sentence and every cause beneath it, so a
//! log line reads as it would have where it was raised.

use std::fmt;

use af_core::error_code::{Coded, ErrorCode};

use super::Error;

/// A failure an engine or a building block raises, crossing the seam.
pub trait EngineFailure: Coded + Send + Sync + 'static {
    /// The seam refusal this failure carries when that is what it is — a host
    /// missing a mechanism, a refused egress plan — so a caller of the seam
    /// still tells it apart.
    fn seam(&self) -> Option<&Error>;
}

/// An [`EngineFailure`] the seam holds: told as itself, its code and sentence its
/// own, its causes beneath it.
pub struct Fault {
    failure: Box<dyn EngineFailure>,
    /// Read once, when the failure crossed, so the seam's code stays a
    /// constant function.
    code: ErrorCode,
}

impl Fault {
    /// Holds `failure`.
    pub(super) fn new(failure: impl EngineFailure) -> Self {
        let code = failure.code();
        Self {
            failure: Box::new(failure),
            code,
        }
    }

    /// The registry code the failure was raised under.
    pub(super) const fn code(&self) -> ErrorCode {
        self.code
    }

    /// The seam refusal the failure carries, when it carries one.
    pub(super) fn seam(&self) -> Option<&Error> {
        self.failure.seam()
    }
}

impl fmt::Debug for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.failure, f)
    }
}

impl fmt::Display for Fault {
    /// The failure's sentence alone: the seam's own `[CODE]` leads it, so the
    /// failure's would say the code twice.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.failure)
    }
}

impl std::error::Error for Fault {
    /// What caused the failure: its sentence is this one's already.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failure.source()
    }
}
