//! What stops the agent before or while it serves.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. Nothing here crosses to the host: the agent
//! writes its refusal to the guest's console, which the runner logs as
//! `sandbox_console`, and the guest ends; the run resumes from its checkpoint
//! on a fresh microVM. The codes are the registry's existing ones
//! (`docs/RUST_ERROR_STANDARD.md`).

use af_core::error_code::{self, ErrorCode};

af_core::error_shell!(
    /// The agent's failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way the agent fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The process is not a microVM's first process, so it serves nothing.
    #[error("agentsfleet-guest refuses to start: {why}")]
    NotInit {
        /// What it found instead.
        why: String,
    },

    /// A file system, process or socket call failed.
    #[error("an input/output call failed")]
    Io {
        /// The operating system's reason.
        #[source]
        source: std::io::Error,
    },

    /// The tenant leaf's freezer could not be opened.
    #[error(transparent)]
    Cgroup {
        /// The cgroup's failure.
        source: af_cgroup::Error,
    },

    /// The inner environment or its confinement could not be built.
    #[error(transparent)]
    Confine {
        /// The confinement's failure.
        source: afg_confine::Error,
    },

    /// The agent failed while it served.
    #[error(transparent)]
    Agent {
        /// The agent's failure.
        source: af_guest_wire::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

// Every lift is a `From`, so `?` does the conversion (rule 2 of the
// standard).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    af_cgroup::Error => Cgroup,
    afg_confine::Error => Confine,
    af_guest_wire::Error => Agent,
);

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::NotInit { .. }
            | ErrorKind::Io { .. }
            | ErrorKind::Cgroup { .. }
            | ErrorKind::Confine { .. } => error_code::INTERNAL_OPERATION_FAILED,
            ErrorKind::Agent { source } => source.code(),
        }
    }
}

/// The process is not what the agent must be, for the reason `why` gives.
pub(crate) fn not_init(why: String) -> Error {
    ErrorKind::NotInit { why }.into()
}
