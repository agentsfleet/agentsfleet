//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

use std::process::ExitStatus;

use super::{Error, ErrorKind, ToolboxRefusal};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    tokio::task::JoinError => Task,
    afr_engine::Error => Seam,
);

#[cfg(target_os = "linux")]
af_core::error_lifts!(Error, ErrorKind:
    rustix::io::Errno => System,
);

/// Reports a host program that ended badly, with the tail of its error stream.
/// Public: an engine reports the launcher it runs the same way.
#[must_use]
pub fn program(program: &'static str, status: ExitStatus, stderr: String) -> Error {
    ErrorKind::Program {
        program,
        status,
        stderr,
    }
    .into()
}

/// Refuses a toolbox release whose `what` would not parse, failing verification's
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

/// Refuses a toolbox release that failed verification's `refusal` check, saying
/// what the check found.
pub(crate) fn toolbox_refused(refusal: ToolboxRefusal, detail: impl Into<String>) -> Error {
    ErrorKind::ToolboxRefused {
        refusal,
        detail: detail.into(),
    }
    .into()
}

/// Refuses what a host cannot give a sandbox, in the seam's words.
#[cfg(not(target_os = "linux"))]
pub(crate) fn unconfined(detail: &'static str) -> Error {
    afr_engine::error::unconfined(detail).into()
}
