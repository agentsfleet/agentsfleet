//! What the process inside a sandbox refuses before it serves.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A step of confinement the kernel refused is
//! named, and so are a kernel without Landlock and a process that is not
//! confined. None of it crosses to the host: the guest's agent logs it on the
//! console, and the run resumes on a fresh microVM. The codes are the
//! registry's existing ones (`docs/RUST_ERROR_STANDARD.md`).

use af_core::error_code::{self, ErrorCode};

mod raise;

#[cfg(not(target_os = "linux"))]
pub(crate) use self::raise::refused;
pub(crate) use self::raise::unconfined;

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

    /// The agent inside the sandbox failed.
    #[error("the sandbox's agent failed")]
    Agent {
        /// The agent's failure.
        #[source]
        source: af_guest_wire::Error,
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

    /// The kernel lacks a mechanism every sandbox needs: off Linux, Landlock.
    #[cfg(not(target_os = "linux"))]
    #[error("this kernel cannot confine a sandbox: {missing} is unavailable")]
    Refused {
        /// The missing mechanism, as the capability report names it.
        missing: &'static str,
    },

    /// The process still holds what it must not, so it is not confined.
    #[error("the sandbox is not confined: {detail}")]
    Unconfined {
        /// What survived.
        detail: &'static str,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Agent { source } => source.code(),
            ErrorKind::Io { .. } | ErrorKind::Unconfined { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
            #[cfg(not(target_os = "linux"))]
            ErrorKind::Refused { .. } => error_code::INTERNAL_OPERATION_FAILED,
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

    /// The mechanism this kernel lacks, when the failure is that it cannot
    /// confine a sandbox at all; a Linux kernel lacks none this crate names.
    #[must_use]
    pub const fn missing_mechanism(&self) -> Option<&'static str> {
        match self.kind() {
            #[cfg(not(target_os = "linux"))]
            ErrorKind::Refused { missing } => Some(missing),
            _other => None,
        }
    }
}
