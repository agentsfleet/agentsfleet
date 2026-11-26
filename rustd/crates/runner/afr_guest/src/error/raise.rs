//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    afr_executor::Error => Executor,
    afr_cgroup::Error => Cgroup,
    afr_engine::Error => Seam,
);

#[cfg(target_os = "linux")]
af_core::error_lifts!(Error, ErrorKind:
    rustix::io::Errno => System,
    landlock::RulesetError => Landlock,
    seccompiler::BackendError => SeccompProgram,
    seccompiler::Error => Seccomp,
);

/// Refuses every run on this host, in the seam's words: `missing` is what it
/// lacks.
#[cfg(not(target_os = "linux"))]
pub(crate) fn refused(missing: &'static str) -> Error {
    afr_engine::error::refused(missing).into()
}

/// Refuses to serve from a process that still holds what it must not, in the
/// seam's words.
pub(crate) fn unconfined(detail: &'static str) -> Error {
    afr_engine::error::unconfined(detail).into()
}

/// Refuses a descriptor the sandbox entry was named but did not inherit.
pub(crate) fn not_inherited(descriptor: i32) -> Error {
    ErrorKind::NotInherited { descriptor }.into()
}
