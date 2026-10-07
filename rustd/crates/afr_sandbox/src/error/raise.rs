//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

use std::process::ExitStatus;

use super::{Error, ErrorKind, ToolboxRefusal};

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

/// Reports a cgroup file that could not be read, naming the file.
pub(crate) fn cgroup_unreadable(file: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::CgroupUnreadable { file, source }.into()
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

/// Reports a cgroup that never reached `state` after it was asked to.
pub(crate) fn cgroup_unsettled(path: &std::path::Path, state: &'static str) -> Error {
    ErrorKind::CgroupUnsettled {
        path: path.to_owned(),
        state,
    }
    .into()
}

/// Refuses a toolbox release whose `what` would not parse, failing admission's
/// `refusal` check, and keeps the parser's reason as the cause.
pub(crate) fn toolbox_unreadable<E>(
    refusal: ToolboxRefusal,
    what: &'static str,
) -> impl Fn(E) -> Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    move |source| {
        ErrorKind::ToolboxUnreadable {
            refusal,
            what,
            source: Box::new(source),
        }
        .into()
    }
}

/// Refuses a toolbox release that failed admission's `refusal` check, saying
/// what the check found.
pub(crate) fn toolbox_refused(refusal: ToolboxRefusal, detail: impl Into<String>) -> Error {
    ErrorKind::ToolboxRefused {
        refusal,
        detail: detail.into(),
    }
    .into()
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

/// Refuses a descriptor the sandbox entry was named but did not inherit.
pub(crate) fn not_inherited(descriptor: i32) -> Error {
    ErrorKind::NotInherited { descriptor }.into()
}
