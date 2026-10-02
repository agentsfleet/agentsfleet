//! What the executor and its client refuse, and what they report.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull every `rustd` crate carries: the boxed kind
//! keeps `Result` pointer-sized on the `Ok` path, and the captured backtrace,
//! the `[CODE]` rendering and the self-skipping `source()` are generated.
//!
//! # Which codes, and why none are new
//!
//! A runner failure is read by an operator on the host's journal, never by a
//! tenant or an API client, so it reuses the registry's existing codes the way
//! `afd_bench` does (`docs/RUST_ERROR_STANDARD.md`); the `event` field on the
//! log line says which failure it was. Minting a `UZ-RUN-*` code would publish
//! it in `public/openapi.json` for a condition no client can observe.

use afd_core::error_code::{self, ErrorCode};
use jsonrpsee_types::error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE};

use crate::protocol::{PATH_REFUSED_CODE, UNKNOWN_PROCESS_CODE};

mod raise;

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;

pub(crate) use self::raise::{
    connection_lost, invalid_params, path_refused, refused, unknown_process,
};

afd_core::error_shell!(
    /// An executor failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A filesystem, socket or process call failed.
    #[error("an input/output call failed")]
    Io {
        /// The operating system's reason.
        #[from]
        source: std::io::Error,
    },

    /// The other end of the socket went away.
    #[error("the executor connection closed")]
    ConnectionLost,

    /// The executor answered a call with an error.
    #[error("the executor refused the call ({code}): {message}")]
    Refused {
        /// The JSON-RPC error code it answered with.
        code: i32,
        /// Its message.
        message: String,
    },

    /// A message did not decode into the shape its method names.
    #[error("a message did not decode")]
    Malformed {
        /// The decoder's reason.
        #[from]
        source: serde_json::Error,
    },

    /// Bytes on the wire were not base64.
    #[error("bytes on the wire were not base64")]
    Encoding {
        /// The decoder's reason.
        #[from]
        source: base64::DecodeError,
    },

    /// A line could not be read or written.
    #[error("a message frame could not be read or written")]
    Frame {
        /// The codec's reason.
        #[from]
        source: tokio_util::codec::LinesCodecError,
    },

    /// A path leaves the workspace, or the sandbox will not open it.
    #[error("the path is outside the workspace or not permitted")]
    PathRefused,

    /// A blocking task serving a call did not finish.
    #[error("a blocking task did not finish")]
    Task {
        /// The runtime's reason.
        #[from]
        source: tokio::task::JoinError,
    },

    /// No such process on this executor.
    #[error("no process with that identifier")]
    UnknownProcess,

    /// A call's parameters were well-formed but unusable.
    #[error("{detail}")]
    InvalidParams {
        /// Why, in the caller's terms.
        detail: &'static str,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        error_code::INTERNAL_OPERATION_FAILED
    }

    /// Whether the call named a path outside the workspace.
    #[must_use]
    pub fn is_path_refused(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::PathRefused
                | ErrorKind::Refused {
                    code: PATH_REFUSED_CODE,
                    ..
                }
        )
    }

    /// Whether the call named a process the executor does not have — one that
    /// never existed, or one that already ended.
    #[must_use]
    pub fn is_unknown_process(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::UnknownProcess
                | ErrorKind::Refused {
                    code: UNKNOWN_PROCESS_CODE,
                    ..
                }
        )
    }

    /// Whether the executor, or its sandbox, went away under the call.
    #[must_use]
    pub fn is_connection_lost(&self) -> bool {
        matches!(self.kind(), ErrorKind::ConnectionLost)
    }

    /// What the other end is told: the failure and, when it has one, its
    /// cause — never the registry code or a backtrace, which are this host's.
    pub(crate) fn wire_message(&self) -> String {
        let kind = self.kind();
        std::error::Error::source(kind)
            .map_or_else(|| kind.to_string(), |cause| format!("{kind}: {cause}"))
    }

    /// The JSON-RPC code a refusal of this kind is answered with.
    pub(crate) fn rpc_code(&self) -> i32 {
        match self.kind() {
            ErrorKind::PathRefused => PATH_REFUSED_CODE,
            ErrorKind::UnknownProcess => UNKNOWN_PROCESS_CODE,
            ErrorKind::InvalidParams { .. }
            | ErrorKind::Malformed { .. }
            | ErrorKind::Encoding { .. } => INVALID_PARAMS_CODE,
            _internal => INTERNAL_ERROR_CODE,
        }
    }
}
