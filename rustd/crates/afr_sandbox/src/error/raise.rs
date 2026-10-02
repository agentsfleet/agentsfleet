//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

use std::process::ExitStatus;

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    afr_executor::Error => Executor,
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

/// Refuses to serve from a process that still holds what it must not.
pub(crate) fn unconfined(detail: &'static str) -> Error {
    ErrorKind::Unconfined { detail }.into()
}
