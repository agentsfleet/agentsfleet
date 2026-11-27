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
#[must_use]
pub fn connection_lost() -> Error {
    ErrorKind::ConnectionLost.into()
}

/// The link was found closed and `source` is why its route would not
/// dial it again.
#[must_use]
pub fn redial_failed(source: std::io::Error) -> Error {
    ErrorKind::Redial { source }.into()
}

/// The guest agent did not answer `method` before the call's deadline.
#[must_use]
pub fn unresponsive(method: &'static str) -> Error {
    ErrorKind::Unresponsive { method }.into()
}

/// The guest agent answered with a JSON-RPC error, carrying its message.
#[must_use]
pub fn refused(code: i32, message: &str) -> Error {
    ErrorKind::Refused {
        code,
        message: message.to_owned(),
    }
    .into()
}

/// A path leaves the workspace.
#[must_use]
pub fn path_refused() -> Error {
    ErrorKind::PathRefused.into()
}

/// A file call named something other than a regular file.
#[must_use]
pub fn not_a_file() -> Error {
    ErrorKind::NotAFile.into()
}

/// A file call named something the workspace does not have; `source` is the
/// operating system saying so.
#[must_use]
pub fn not_found(source: std::io::Error) -> Error {
    ErrorKind::NotFound { source }.into()
}

/// No such process.
#[must_use]
pub fn unknown_process() -> Error {
    ErrorKind::UnknownProcess.into()
}

/// A process's input queue is full.
#[must_use]
pub fn input_backlog_full() -> Error {
    ErrorKind::InputBacklogFull.into()
}

/// A process's input is closed.
#[must_use]
pub fn input_closed() -> Error {
    ErrorKind::InputClosed.into()
}

/// A process started without a handle the guest agent needs.
#[must_use]
pub fn launch_incomplete() -> Error {
    ErrorKind::LaunchIncomplete.into()
}

/// The tenant leaf cannot be entered, for the reason `source` gives.
#[must_use]
pub fn tenant_unavailable(source: std::io::Error) -> Error {
    ErrorKind::TenantUnavailable { source }.into()
}

/// A freeze or thaw to a guest agent with no tenant leaf.
#[must_use]
pub fn no_tenant_leaf() -> Error {
    ErrorKind::NoTenantLeaf.into()
}

/// The tenant leaf never reached `state`, for the reason `source` gives.
pub fn tenant_unfrozen(state: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::TenantUnfrozen { state, source }.into()
}

/// A file call to a guest agent that serves none.
#[must_use]
pub fn file_calls_unsupported() -> Error {
    ErrorKind::FileCallsUnsupported.into()
}

/// A renewal or a hosts file to a guest agent that builds no inner environment.
#[must_use]
pub fn no_inner_environment() -> Error {
    ErrorKind::NoInnerEnvironment.into()
}

/// The inner environment could not be entered, torn down or built again,
/// for the reason `source` gives.
#[must_use]
pub fn inner_unavailable(source: std::io::Error) -> Error {
    ErrorKind::InnerUnavailable { source }.into()
}

/// Parameters that decoded but cannot be used.
#[must_use]
pub fn invalid_params(detail: &'static str) -> Error {
    ErrorKind::InvalidParams { detail }.into()
}
