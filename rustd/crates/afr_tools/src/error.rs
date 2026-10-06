//! What the catalog refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull: the refusal carries the tool's name, so it
//! takes the boxed kind, the backtrace and the `[CODE]` rendering. Nothing
//! here wraps a cause, so there is no `error_lifts!`.
//!
//! # Which code, and why none is new
//!
//! A refused lease is read by an operator on the host's journal, never by a
//! tenant, so it reuses the registry's code for a fleet whose configuration
//! cannot run, the way the rest of the runner reuses codes
//! (`docs/RUST_ERROR_STANDARD.md`). The log line's `event` says which refusal.

use afd_core::error_code::{self, ErrorCode};

afd_core::error_shell!(
    /// A catalog refusal, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way the catalog refuses.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The lease's policy names a tool this runner cannot host.
    #[error("the policy names a tool this runner cannot host: {name}")]
    Unhosted {
        /// The tool's name, as the policy spells it.
        name: String,
    },

    /// The policy binds a repository whose name is not `owner/name`.
    #[error("the policy binds a repository this runner cannot check out: {name}")]
    InvalidRepository {
        /// The repository, as the policy spells it.
        name: String,
    },

    /// The policy binds two repositories that would land in one directory.
    #[error(
        "the policy binds two repositories this runner would check out in one directory: \
         {first} and {second}"
    )]
    SharedDirectory {
        /// The first of the two, as the policy spells it.
        first: String,
        /// The second.
        second: String,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this refusal is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Unhosted { .. }
            | ErrorKind::InvalidRepository { .. }
            | ErrorKind::SharedDirectory { .. } => error_code::AGENTSFLEET_INVALID_CONFIG,
        }
    }

    /// The tool a refused lease named, for its log line.
    #[must_use]
    pub fn unhosted_tool(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Unhosted { name } => Some(name),
            ErrorKind::InvalidRepository { .. } | ErrorKind::SharedDirectory { .. } => None,
        }
    }
}

/// A policy binding `name`, which is not a repository this runner can check
/// out.
pub(crate) fn invalid_repository(name: &str) -> Error {
    Error::from(ErrorKind::InvalidRepository {
        name: name.to_owned(),
    })
}

/// A policy binding `first` and `second`, which would land in one directory.
pub(crate) fn shared_directory(first: &str, second: &str) -> Error {
    Error::from(ErrorKind::SharedDirectory {
        first: first.to_owned(),
        second: second.to_owned(),
    })
}

/// A policy naming `name`, which this runner cannot host.
pub(crate) fn unhosted(name: &str) -> Error {
    Error::from(ErrorKind::Unhosted {
        name: name.to_owned(),
    })
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
