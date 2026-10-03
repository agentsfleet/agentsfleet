//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

use std::process::ExitStatus;

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    afr_executor::Error => Executor,
    tokio::task::JoinError => Task,
);

#[cfg(target_os = "linux")]
afd_core::error_lifts!(Error, ErrorKind:
    rustix::io::Errno => System,
    landlock::RulesetError => Landlock,
    seccompiler::BackendError => SeccompProgram,
    seccompiler::Error => Seccomp,
);

/// Refuses every lease on this host: `missing` is what it lacks.
pub(crate) fn refused(missing: &'static str) -> Error {
    ErrorKind::Refused { missing }.into()
}

/// Reports a host program that ended badly, with the tail of its error stream.
pub(crate) fn program(program: &'static str, status: ExitStatus, stderr: String) -> Error {
    ErrorKind::Program {
        program,
        status,
        stderr,
    }
    .into()
}

/// Reports a cgroup control file that refused a write, naming the file.
pub(crate) fn cgroup(file: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::Cgroup { file, source }.into()
}

/// Reports a cgroup that would not go, naming it.
pub(crate) fn cgroup_left(path: &std::path::Path) -> impl Fn(std::io::Error) -> Error {
    move |source| {
        ErrorKind::CgroupLeft {
            path: path.to_owned(),
            source,
        }
        .into()
    }
}

/// Refuses a toolbox image whose name states no digest.
pub(crate) fn toolbox_unnamed(path: &std::path::Path) -> Error {
    ErrorKind::ToolboxUnnamed {
        path: path.to_owned(),
    }
    .into()
}

/// Refuses a toolbox image, or the device it is mounted from, whose bytes
/// hash to `actual` rather than the digest it is named by.
pub(crate) fn toolbox_unverified(path: &std::path::Path, actual: String) -> Error {
    ErrorKind::ToolboxUnverified {
        path: path.to_owned(),
        actual,
    }
    .into()
}

/// Refuses a toolbox root not mounted from a named loop device.
#[cfg(target_os = "linux")]
pub(crate) fn toolbox_device(major: u32, minor: u32) -> Error {
    ErrorKind::ToolboxDevice { major, minor }.into()
}

/// Reports an executor not reached within `waited`, with why.
#[cfg(target_os = "linux")]
pub(crate) fn not_ready(waited: std::time::Duration) -> impl Fn(afr_executor::Error) -> Error {
    move |source| ErrorKind::NotReady { waited, source }.into()
}

/// Refuses a toolbox other than the one this runner was configured for.
#[cfg(target_os = "linux")]
pub(crate) fn toolbox_unexpected(actual: &str, expected: &str) -> Error {
    ErrorKind::ToolboxUnexpected {
        actual: actual.to_owned(),
        expected: expected.to_owned(),
    }
    .into()
}

/// Refuses a lease whose identifier could name a directory not its own.
pub(crate) fn lease_id_unsafe(lease_id: &str) -> Error {
    ErrorKind::LeaseIdUnsafe {
        lease_id: lease_id.to_owned(),
    }
    .into()
}

/// Refuses to serve from a process that still holds what it must not.
pub(crate) fn unconfined(detail: &'static str) -> Error {
    ErrorKind::Unconfined { detail }.into()
}
