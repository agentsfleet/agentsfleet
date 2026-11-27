//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    afr_executor::Error => Executor,
    afr_engine::Error => Seam,
);

#[cfg(target_os = "linux")]
af_core::error_lifts!(Error, ErrorKind:
    rustix::io::Errno => System,
    landlock::RulesetError => Landlock,
    seccompiler::BackendError => SeccompProgram,
    seccompiler::Error => Seccomp,
);

/// A confinement's refused step is the failure the refusing call always
/// was: the kernel's reason, Landlock's or seccomp's, or the seam's refusal
/// of a process that still holds what it must not. Written out because one
/// source lifts into four kinds.
#[cfg(target_os = "linux")]
impl From<crate::harden::Refused> for Error {
    fn from(refused: crate::harden::Refused) -> Self {
        use crate::harden::{CAPABILITY_SURVIVED, Refused};

        match refused {
            Refused::System(source) => source.into(),
            Refused::Landlock(source) => source.into(),
            Refused::Seccomp(source) => source.into(),
            Refused::Capability => unconfined(CAPABILITY_SURVIVED),
            Refused::Spent => unconfined(CONFINEMENT_SPENT),
        }
    }
}

/// Why a confinement asked to apply a second time is refused.
#[cfg(target_os = "linux")]
const CONFINEMENT_SPENT: &str = "the confinement was applied already";

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
