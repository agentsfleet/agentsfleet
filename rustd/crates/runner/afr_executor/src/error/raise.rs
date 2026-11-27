//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind data.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    serde_json::Error => Malformed,
    tokio::task::JoinError => Task,
);

/// The other end of the socket went away.
pub(crate) fn connection_lost() -> Error {
    ErrorKind::ConnectionLost.into()
}

/// The link was found closed and `source` is why its route would not
/// dial it again.
pub(crate) fn redial_failed(source: std::io::Error) -> Error {
    ErrorKind::Redial { source }.into()
}

/// The executor did not answer `method` before the call's deadline.
pub(crate) fn unresponsive(method: &'static str) -> Error {
    ErrorKind::Unresponsive { method }.into()
}

/// The executor answered with a JSON-RPC error, carrying its message.
pub(crate) fn refused(code: i32, message: &str) -> Error {
    ErrorKind::Refused {
        code,
        message: message.to_owned(),
    }
    .into()
}

/// A path leaves the workspace.
pub(crate) fn path_refused() -> Error {
    ErrorKind::PathRefused.into()
}

/// A file call named something other than a regular file.
pub(crate) fn not_a_file() -> Error {
    ErrorKind::NotAFile.into()
}

/// A file call named something the workspace does not have; `source` is the
/// operating system saying so.
pub(crate) fn not_found(source: std::io::Error) -> Error {
    ErrorKind::NotFound { source }.into()
}

/// No such process.
pub(crate) fn unknown_process() -> Error {
    ErrorKind::UnknownProcess.into()
}

/// A process's input queue is full.
pub(crate) fn input_backlog_full() -> Error {
    ErrorKind::InputBacklogFull.into()
}

/// A process's input is closed.
pub(crate) fn input_closed() -> Error {
    ErrorKind::InputClosed.into()
}

/// A process started without a handle the executor needs.
pub(crate) fn launch_incomplete() -> Error {
    ErrorKind::LaunchIncomplete.into()
}

/// The tenant leaf cannot be entered, for the reason `source` gives.
pub(crate) fn tenant_unavailable(source: std::io::Error) -> Error {
    ErrorKind::TenantUnavailable { source }.into()
}

/// A freeze or thaw to an executor with no tenant leaf.
pub(crate) fn no_tenant_leaf() -> Error {
    ErrorKind::NoTenantLeaf.into()
}

/// The tenant leaf never reached `state`, for the reason `source` gives.
pub(crate) fn tenant_unfrozen(state: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::TenantUnfrozen { state, source }.into()
}

/// A file call to an executor that serves none.
pub(crate) fn file_calls_unsupported() -> Error {
    ErrorKind::FileCallsUnsupported.into()
}

/// A renewal or a hosts file to an executor that builds no inner environment.
pub(crate) fn no_inner_environment() -> Error {
    ErrorKind::NoInnerEnvironment.into()
}

/// The inner environment could not be entered, torn down or built again,
/// for the reason `source` gives.
pub(crate) fn inner_unavailable(source: std::io::Error) -> Error {
    ErrorKind::InnerUnavailable { source }.into()
}

/// Parameters that decoded but cannot be used.
pub(crate) fn invalid_params(detail: &'static str) -> Error {
    ErrorKind::InvalidParams { detail }.into()
}
