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

/// No such process.
pub(crate) fn unknown_process() -> Error {
    ErrorKind::UnknownProcess.into()
}

/// Parameters that decoded but cannot be used.
pub(crate) fn invalid_params(detail: &'static str) -> Error {
    ErrorKind::InvalidParams { detail }.into()
}
