//! What verifying a toolbox, or making, mounting or removing a workspace
//! image, refuses.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A toolbox release that fails a check names
//! its [`ToolboxRefusal`]; a host program that exits badly names itself; a
//! host with no loop device is the seam's refusal, carried as itself. The
//! codes are the registry's existing ones, as every runner crate's are
//! (`docs/RUST_ERROR_STANDARD.md`).

use std::process::ExitStatus;

use af_core::error_code::{self, ErrorCode};

mod raise;
mod toolbox;

pub use self::raise::program;
pub(crate) use self::raise::{toolbox_refused, toolbox_unreadable};
pub use self::toolbox::ToolboxRefusal;

af_core::error_shell!(
    /// An image failure, with the backtrace of where it was raised.
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

    /// A toolbox release failed one of verification's checks, before anything
    /// of it was mounted.
    #[error("the toolbox release was refused ({refusal}): {detail}")]
    ToolboxRefused {
        /// Which check it failed.
        refusal: ToolboxRefusal,
        /// What the check found.
        detail: String,
    },

    /// A toolbox release's key, signature or manifest could not be read.
    #[error("the toolbox release was refused ({refusal}): {what} could not be read")]
    ToolboxUnreadable {
        /// Which check it failed.
        refusal: ToolboxRefusal,
        /// What could not be read.
        what: &'static str,
        /// The parser's reason.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A host program the engine runs exited unsuccessfully.
    #[error("{program} exited with {status}: {stderr}")]
    Program {
        /// Which program.
        program: &'static str,
        /// How it ended.
        status: ExitStatus,
        /// The last of what it wrote to standard error.
        stderr: String,
    },

    /// A task the image ran off the runtime did not finish: it panicked or
    /// was cancelled.
    #[error("a task did not finish")]
    Task {
        /// The runtime's reason.
        #[source]
        source: tokio::task::JoinError,
    },

    /// A system call made directly was refused.
    #[cfg(target_os = "linux")]
    #[error("a system call was refused")]
    System {
        /// The kernel's reason.
        #[source]
        source: rustix::io::Errno,
    },

    /// A refusal in the seam's words: a host with no loop device to give.
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
            | ErrorKind::ToolboxRefused { .. }
            | ErrorKind::ToolboxUnreadable { .. }
            | ErrorKind::Program { .. }
            | ErrorKind::Task { .. } => error_code::INTERNAL_OPERATION_FAILED,
            #[cfg(target_os = "linux")]
            ErrorKind::System { .. } => error_code::INTERNAL_OPERATION_FAILED,
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
            | ErrorKind::ToolboxRefused { .. }
            | ErrorKind::ToolboxUnreadable { .. }
            | ErrorKind::Program { .. }
            | ErrorKind::Task { .. } => None,
            #[cfg(target_os = "linux")]
            ErrorKind::System { .. } => None,
        }
    }

    /// Which verification check a toolbox release failed, when that is the
    /// failure.
    #[must_use]
    pub const fn toolbox_refusal(&self) -> Option<ToolboxRefusal> {
        match self.kind() {
            ErrorKind::ToolboxRefused { refusal, .. }
            | ErrorKind::ToolboxUnreadable { refusal, .. } => Some(*refusal),
            ErrorKind::Io { .. }
            | ErrorKind::Program { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::Seam { .. } => None,
            #[cfg(target_os = "linux")]
            ErrorKind::System { .. } => None,
        }
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
