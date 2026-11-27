//! What a cgroup file or a freeze refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A control file that refuses a write or a
//! read is named, and so is a tree that never settled. The codes are the
//! registry's existing ones (`docs/RUST_ERROR_STANDARD.md`): an operator
//! reads them on the host's journal or the guest's console, never a client.

use std::path::PathBuf;

use af_core::error_code::{self, ErrorCode};

mod raise;

pub(crate) use self::raise::cgroup_unsettled;
pub use self::raise::{cgroup, cgroup_unreadable};

af_core::error_shell!(
    /// A cgroup failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A cgroup control file refused a write.
    #[error("the cgroup refused a write to {file}")]
    Cgroup {
        /// Which control file.
        file: &'static str,
        /// The kernel's reason.
        #[source]
        source: std::io::Error,
    },

    /// A cgroup file could not be opened or read.
    #[error("the cgroup file {file} could not be read")]
    CgroupUnreadable {
        /// Which file.
        file: &'static str,
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
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Cgroup { .. }
            | ErrorKind::CgroupUnreadable { .. }
            | ErrorKind::CgroupUnsettled { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The failure in its own sentence, without its code or backtrace.
    #[must_use]
    pub fn detail(&self) -> String {
        format!("{:#}", self.kind())
    }
}

#[cfg(test)]
mod tests;
