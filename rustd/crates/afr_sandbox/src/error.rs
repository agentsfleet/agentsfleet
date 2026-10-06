//! What building or destroying a sandbox refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull every `rustd` crate carries: the boxed kind
//! keeps `Result` pointer-sized on the `Ok` path, and the captured backtrace,
//! the `[CODE]` rendering and the self-skipping `source()` are generated.
//!
//! # Which codes, and why none are new
//!
//! A runner failure is read by an operator on the host's journal, never by a
//! tenant or an API client, so it reuses the registry's existing codes the way
//! `afd_bench` does (`docs/RUST_ERROR_STANDARD.md`); the `event` field on the
//! log line says which failure it was. Minting a `UZ-RUN-*` code would publish
//! it in `public/openapi.json` for a condition no client can observe.

use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus;
#[cfg(target_os = "linux")]
use std::time::Duration;

use afd_core::error_code::{self, ErrorCode};

mod raise;

pub(crate) use self::raise::{
    cgroup, cgroup_left, lease_id_unsafe, program, refused, toolbox_refused, toolbox_unreadable,
    unconfined,
};
#[cfg(target_os = "linux")]
pub(crate) use self::raise::{not_ready, toolbox_unexpected};

afd_core::error_shell!(
    /// A sandbox failure, with the backtrace of where it was raised.
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

    /// The host lacks something every sandbox needs, so no lease runs here.
    #[error("this host cannot build a sandbox: {missing} is unavailable")]
    Refused {
        /// The missing mechanism, as the capability report names it.
        missing: &'static str,
    },

    /// A toolbox release failed one of admission's checks, before anything
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

    /// A cgroup control file refused a write.
    #[error("the cgroup refused a write to {file}")]
    Cgroup {
        /// Which control file.
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

    /// The toolbox is not the image this runner was configured to run.
    #[cfg(target_os = "linux")]
    #[error("the toolbox is {actual}, but this runner is configured for {expected}")]
    ToolboxUnexpected {
        /// The digest of the image that is mounted.
        actual: String,
        /// The digest the configuration names.
        expected: String,
    },

    /// The executor could not be reached within the ready timeout.
    #[cfg(target_os = "linux")]
    #[error("the sandbox's executor did not answer within {waited:?}")]
    NotReady {
        /// How long the engine waited.
        waited: Duration,
        /// Why the last attempt failed: a socket not there yet, or a refusal
        /// waiting could not fix.
        #[source]
        source: afr_executor::Error,
    },

    /// A task the engine ran did not finish: it panicked or was cancelled.
    #[error("a task did not finish")]
    Task {
        /// The runtime's reason.
        #[source]
        source: tokio::task::JoinError,
    },

    /// The executor inside the sandbox failed.
    #[error("the sandbox's executor failed")]
    Executor {
        /// The executor's failure.
        #[source]
        source: afr_executor::Error,
    },

    /// A hardened process still holds something it must not.
    #[error("the sandbox is not confined: {detail}")]
    Unconfined {
        /// What remained.
        detail: &'static str,
    },

    /// A lease identifier that is not one plain path component.
    #[error("the lease identifier {lease_id:?} is not a single path component")]
    LeaseIdUnsafe {
        /// The identifier as it arrived.
        lease_id: String,
    },

    /// The unsandboxed engine was asked for in a release build.
    #[error("a release build never runs a tool call unsandboxed")]
    UnsandboxedInRelease,

    /// A system call the engine makes directly was refused.
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
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Executor { source } => source.code(),
            _local => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The mechanism this host lacks, when the failure is that the host cannot
    /// build a sandbox at all — as opposed to one sandbox failing.
    #[must_use]
    pub fn missing_mechanism(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Refused { missing } => Some(missing),
            _built => None,
        }
    }

    /// Which admission check a toolbox release failed, when that is the
    /// failure.
    #[must_use]
    pub fn toolbox_refusal(&self) -> Option<ToolboxRefusal> {
        match self.kind() {
            ErrorKind::ToolboxRefused { refusal, .. }
            | ErrorKind::ToolboxUnreadable { refusal, .. } => Some(*refusal),
            _other => None,
        }
    }
}

/// Why a toolbox release was not admitted: one value per check, so a log line,
/// the capability report and a test tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolboxRefusal {
    /// The manifest's signature is not the release key's over its exact bytes.
    Signature,
    /// The manifest is not one admission can read.
    Manifest,
    /// The release is built for another architecture.
    Architecture,
    /// The release does not serve this runner's version.
    RunnerVersion,
    /// The release uses an EROFS feature this runner does not admit.
    Features,
    /// The image is not the length the manifest names.
    Length,
    /// The image's bytes do not hash to the manifest's digest.
    Digest,
    /// The image is not a regular file: a link, a directory or a device.
    NotAFile,
}

impl ToolboxRefusal {
    /// How a log line spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Signature => "signature_invalid",
            Self::Manifest => "manifest_invalid",
            Self::Architecture => "architecture_mismatch",
            Self::RunnerVersion => "runner_unserved",
            Self::Features => "features_unsupported",
            Self::Length => "length_mismatch",
            Self::Digest => "digest_mismatch",
            Self::NotAFile => "not_a_file",
        }
    }
}

impl fmt::Display for ToolboxRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
