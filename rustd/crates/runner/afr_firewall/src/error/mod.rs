//! What building, holding or removing an egress scope refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A netlink request the kernel refused names
//! its [`Step`]; a host that cannot hold a sandbox to an allowlist is the
//! seam's [`afr_engine::EgressRefusal`], carried as itself, so a caller of
//! the seam reads which one it was. The codes are the registry's existing
//! ones, as every runner crate's are (`docs/RUST_ERROR_STANDARD.md`).

use af_core::error_code::{self, ErrorCode};
use afr_engine::EgressRefusal;

mod raise;
#[cfg(target_os = "linux")]
mod step;

#[cfg(target_os = "linux")]
pub(crate) use self::raise::{egress_refused, netlink};
#[cfg(target_os = "linux")]
pub use self::step::Step;

af_core::error_shell!(
    /// An egress failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A filesystem or socket call failed.
    #[error("an input/output call failed")]
    Io {
        /// The operating system's reason.
        #[source]
        source: std::io::Error,
    },

    /// A netlink request that builds or removes an egress scope failed.
    #[cfg(target_os = "linux")]
    #[error("the kernel refused {step}")]
    Netlink {
        /// What was asked of it.
        step: Step,
        /// The kernel's reason.
        #[source]
        source: std::io::Error,
    },

    /// A refusal in the seam's words: a run's egress that cannot be held to
    /// its policy.
    #[error(transparent)]
    Seam {
        /// The seam's refusal.
        source: afr_engine::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Seam { source } => source.code(),
            ErrorKind::Io { .. } => error_code::INTERNAL_OPERATION_FAILED,
            #[cfg(target_os = "linux")]
            ErrorKind::Netlink { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The failure in its own sentence, without its code or backtrace: a
    /// kind told as the error it carries is told as that error's sentence.
    #[must_use]
    pub fn detail(&self) -> String {
        format!("{:#}", self.kind())
    }

    /// The seam refusal this failure is, when it is one.
    #[must_use]
    pub const fn seam(&self) -> Option<&afr_engine::Error> {
        match self.kind() {
            ErrorKind::Seam { source } => Some(source),
            ErrorKind::Io { .. } => None,
            #[cfg(target_os = "linux")]
            ErrorKind::Netlink { .. } => None,
        }
    }

    /// Why the run's egress was refused, when that is the failure.
    #[must_use]
    pub fn egress_refusal(&self) -> Option<&EgressRefusal> {
        self.seam().and_then(afr_engine::Error::egress_refusal)
    }

    /// Which netlink step the kernel refused, when that is the failure.
    #[cfg(target_os = "linux")]
    #[must_use]
    pub const fn netlink_step(&self) -> Option<Step> {
        match self.kind() {
            ErrorKind::Netlink { step, .. } => Some(*step),
            _other => None,
        }
    }
}

impl afr_engine::Failure for Error {
    fn seam(&self) -> Option<&afr_engine::Error> {
        Self::seam(self)
    }
}

impl From<Error> for afr_engine::Error {
    fn from(error: Error) -> Self {
        Self::engine(error)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
