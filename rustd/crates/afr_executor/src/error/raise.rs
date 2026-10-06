//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind data.

use super::{Error, ErrorKind};
use crate::protocol::REFUSAL_MESSAGE_MAX_BYTES;

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    serde_json::Error => Malformed,
    tokio::task::JoinError => Task,
);

/// The other end of the socket went away.
pub(crate) fn connection_lost() -> Error {
    ErrorKind::ConnectionLost.into()
}

/// The executor did not answer `method` before the call's deadline.
pub(crate) fn unresponsive(method: &'static str) -> Error {
    ErrorKind::Unresponsive { method }.into()
}

/// The executor answered with a JSON-RPC error. Its message is kept up to
/// `REFUSAL_MESSAGE_MAX_BYTES`, cut on a character boundary: the other end of
/// the socket is not trusted with the length of what a model reads.
pub(crate) fn refused(code: i32, message: &str) -> Error {
    let kept = message.floor_char_boundary(REFUSAL_MESSAGE_MAX_BYTES);
    ErrorKind::Refused {
        code,
        message: message.get(..kept).unwrap_or_default().to_owned(),
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

/// The launcher could not find or start the program, for the reason `source`
/// gives.
pub(crate) fn program_unavailable(
    source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> Error {
    ErrorKind::ProgramUnavailable {
        source: source.into(),
    }
    .into()
}

/// A process started without a handle the executor needs.
pub(crate) fn launch_incomplete() -> Error {
    ErrorKind::LaunchIncomplete.into()
}

/// Parameters that decoded but cannot be used.
pub(crate) fn invalid_params(detail: &'static str) -> Error {
    ErrorKind::InvalidParams { detail }.into()
}
