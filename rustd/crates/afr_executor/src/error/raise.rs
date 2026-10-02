//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind data.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    serde_json::Error => Malformed,
    base64::DecodeError => Encoding,
    tokio_util::codec::LinesCodecError => Frame,
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

/// The executor answered with a JSON-RPC error.
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

/// No such process.
pub(crate) fn unknown_process() -> Error {
    ErrorKind::UnknownProcess.into()
}

/// A process's input queue is full.
pub(crate) fn input_backlog_full() -> Error {
    ErrorKind::InputBacklogFull.into()
}

/// The launcher could not find or start the program.
pub(crate) fn program_unavailable(reason: String) -> Error {
    ErrorKind::ProgramUnavailable { reason }.into()
}

/// Parameters that decoded but cannot be used.
pub(crate) fn invalid_params(detail: &'static str) -> Error {
    ErrorKind::InvalidParams { detail }.into()
}
