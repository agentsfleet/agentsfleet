//! What the process inside a sandbox refuses before it serves.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A step of confinement the kernel refused is
//! named; a host without Landlock and a process that is not confined are the
//! seam's refusals, carried as themselves. The codes are the registry's
//! existing ones, as every runner crate's are (`docs/RUST_ERROR_STANDARD.md`).

use af_core::error_code::{self, ErrorCode};

mod raise;

#[cfg(not(target_os = "linux"))]
pub(crate) use self::raise::refused;
pub(crate) use self::raise::{not_inherited, unconfined};

af_core::error_shell!(
    /// A sandbox entry's failure, with the backtrace of where it was raised.
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

    /// The executor inside the sandbox failed.
    #[error("the sandbox's executor failed")]
    Executor {
        /// The executor's failure.
        #[source]
        source: afr_executor::Error,
    },

    /// The sandbox entry was named a descriptor it did not inherit, or the
    /// same one twice.
    #[error("descriptor {descriptor} was not inherited from the engine")]
    NotInherited {
        /// The number the command line named.
        descriptor: i32,
    },

    /// A tenant leaf file the engine opens refused.
    #[error(transparent)]
    Cgroup {
        /// The cgroup's failure.
        source: afr_cgroup::Error,
    },

    /// A system call made directly was refused.
    #[cfg(target_os = "linux")]
    #[error("a system call was refused")]
    System {
        /// The kernel's reason.
        #[source]
        source: rustix::io::Errno,
    },

    /// Landlock refused the ruleset.
    #[cfg(target_os = "linux")]
    #[error("Landlock refused the file-system ruleset")]
    Landlock {
        /// Landlock's reason.
        #[source]
        source: landlock::RulesetError,
    },

    /// The seccomp program would not compile.
    #[cfg(target_os = "linux")]
    #[error("the seccomp program would not compile")]
    SeccompProgram {
        /// The compiler's reason.
        #[source]
        source: seccompiler::BackendError,
    },

    /// The kernel refused the seccomp program.
    #[cfg(target_os = "linux")]
    #[error("the kernel refused the seccomp program")]
    Seccomp {
        /// The kernel's reason.
        #[source]
        source: seccompiler::Error,
    },

    /// A refusal in the seam's words: a host without Landlock, a process
    /// that still holds what it must not.
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
            ErrorKind::Executor { source } => source.code(),
            ErrorKind::Cgroup { source } => source.code(),
            ErrorKind::Seam { source } => source.code(),
            ErrorKind::Io { .. } | ErrorKind::NotInherited { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
            #[cfg(target_os = "linux")]
            ErrorKind::System { .. }
            | ErrorKind::Landlock { .. }
            | ErrorKind::SeccompProgram { .. }
            | ErrorKind::Seccomp { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The failure in its own sentence, without its code or backtrace: a
    /// kind told as the error it carries is told as that error's sentence.
    #[must_use]
    pub fn detail(&self) -> String {
        format!("{:#}", self.kind())
    }

    /// The seam refusal this failure is or carries, when it is one.
    #[must_use]
    pub const fn seam(&self) -> Option<&afr_engine::Error> {
        match self.kind() {
            ErrorKind::Seam { source } => Some(source),
            ErrorKind::Cgroup { source } => source.seam(),
            ErrorKind::Io { .. } | ErrorKind::Executor { .. } | ErrorKind::NotInherited { .. } => {
                None
            }
            #[cfg(target_os = "linux")]
            ErrorKind::System { .. }
            | ErrorKind::Landlock { .. }
            | ErrorKind::SeccompProgram { .. }
            | ErrorKind::Seccomp { .. } => None,
        }
    }

    /// The mechanism this host lacks, when the failure is that the host cannot
    /// build a sandbox at all.
    #[must_use]
    pub fn missing_mechanism(&self) -> Option<&'static str> {
        self.seam().and_then(afr_engine::Error::missing_mechanism)
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
