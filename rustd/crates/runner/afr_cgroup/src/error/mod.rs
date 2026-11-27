//! What a run's cgroup refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A control file that refuses a write or a
//! read is named; a host missing a controller is the seam's refusal, carried
//! as itself, so the supervisor reads which mechanism it lacks. The codes are
//! the registry's existing ones, as every runner crate's are
//! (`docs/RUST_ERROR_STANDARD.md`).

use std::path::PathBuf;

use af_core::error_code::{self, ErrorCode};

mod raise;

pub use self::raise::{cgroup, cgroup_unreadable};
pub(crate) use self::raise::{cgroup_left, cgroup_unsettled};

af_core::error_shell!(
    /// A cgroup failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A filesystem call failed.
    #[error("an input/output call failed")]
    Io {
        /// The operating system's reason.
        #[source]
        source: std::io::Error,
    },

    /// A cgroup control file refused a write.
    #[error("the cgroup refused a write to {file}")]
    Cgroup {
        /// Which control file.
        file: &'static str,
        /// The kernel's reason.
        #[source]
        source: std::io::Error,
    },

    /// A cgroup file the engine reads could not be opened or read.
    #[error("the cgroup file {file} could not be read")]
    CgroupUnreadable {
        /// Which file.
        file: &'static str,
        /// The kernel's reason.
        #[source]
        source: std::io::Error,
    },

    /// A cgroup could not be removed: still busy after its kill, or refused.
    #[error("the cgroup {path} could not be removed")]
    CgroupLeft {
        /// The cgroup's directory.
        path: PathBuf,
        /// The kernel's reason.
        #[source]
        source: std::io::Error,
    },

    /// A cgroup took its freeze or thaw but never reported reaching it, so
    /// some process in it is still running or still stopped.
    #[error("the cgroup {path} did not settle {state}")]
    CgroupUnsettled {
        /// The cgroup's directory.
        path: PathBuf,
        /// The state it was asked for: `frozen` or `thawed`.
        state: &'static str,
    },

    /// A refusal in the seam's words: a host missing a controller, a tree
    /// that is not confined.
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
            ErrorKind::Io { .. }
            | ErrorKind::Cgroup { .. }
            | ErrorKind::CgroupUnreadable { .. }
            | ErrorKind::CgroupLeft { .. }
            | ErrorKind::CgroupUnsettled { .. } => error_code::INTERNAL_OPERATION_FAILED,
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
            ErrorKind::Io { .. }
            | ErrorKind::Cgroup { .. }
            | ErrorKind::CgroupUnreadable { .. }
            | ErrorKind::CgroupLeft { .. }
            | ErrorKind::CgroupUnsettled { .. } => None,
        }
    }

    /// The mechanism this host lacks, when the failure is that the host cannot
    /// build a sandbox at all.
    #[must_use]
    pub fn missing_mechanism(&self) -> Option<&'static str> {
        self.seam().and_then(afr_engine::Error::missing_mechanism)
    }
}

impl afr_engine::EngineFailure for Error {
    fn seam(&self) -> Option<&afr_engine::Error> {
        Self::seam(self)
    }
}

impl From<Error> for afr_engine::Error {
    fn from(error: Error) -> Self {
        Self::engine(error)
    }
}

#[cfg(test)]
mod tests;
