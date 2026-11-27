//! What the guest agent and its client refuse, and what they report.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull every `rustd` crate carries: the boxed kind
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
//!
//! # What the other end of the socket is told
//!
//! [`Error::rpc_code`] sorts a failure into the caller's mistake (invalid
//! params), a refusal the guest agent names (a path outside the workspace, a
//! process it does not have, a full input queue) and its own fault (internal
//! error), so a model reading the answer can tell which it can fix.

use std::io;

use af_core::error_code::{self, ErrorCode};
use jsonrpsee_types::error::{
    CALL_EXECUTION_FAILED_CODE, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE,
};

use crate::protocol::{
    FILE_NOT_FOUND_CODE, NO_TENANT_LEAF_CODE, PATH_REFUSED_CODE, UNKNOWN_PROCESS_CODE,
};

mod raise;

#[cfg(test)]
mod tests;

pub use self::raise::{
    connection_lost, file_calls_unsupported, inner_unavailable, input_backlog_full, input_closed,
    invalid_params, launch_incomplete, no_inner_environment, no_tenant_leaf, not_a_file, not_found,
    path_refused, redial_failed, refused, tenant_unavailable, tenant_unfrozen, unknown_process,
    unresponsive,
};

af_core::error_shell!(
    /// A guest agent failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A filesystem, socket or process call failed.
    #[error("an input/output call failed")]
    Io {
        /// The operating system's reason.
        #[source]
        source: io::Error,
    },

    /// The other end of the socket went away.
    #[error("the guest agent connection closed")]
    ConnectionLost,

    /// The link was found closed and its route would not dial again.
    #[error("the guest agent connection closed and could not be dialled again")]
    Redial {
        /// The route's reason.
        #[source]
        source: io::Error,
    },

    /// The guest agent did not answer in time, so its connection is given up.
    #[error("the guest agent did not answer {method} in time")]
    Unresponsive {
        /// The call that went unanswered.
        method: &'static str,
    },

    /// The guest agent answered a call with an error.
    #[error("the guest agent refused the call ({code}): {message}")]
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
        #[source]
        source: serde_json::Error,
    },

    /// A task the guest agent ran did not finish: it panicked or was cancelled.
    #[error("a task did not finish")]
    Task {
        /// The runtime's reason.
        #[source]
        source: tokio::task::JoinError,
    },

    /// A path leaves the workspace.
    #[error("the path is outside the workspace")]
    PathRefused,

    /// A file call named something that is not a regular file — a pipe, a
    /// socket, a device — which could block the guest agent or reach past it.
    #[error("the path is not a regular file")]
    NotAFile,

    /// A file call named a file or directory the workspace does not have:
    /// the one mistake of the caller's a handler names to the model.
    #[error("the workspace has no such file or directory")]
    NotFound {
        /// The operating system's reason.
        #[source]
        source: io::Error,
    },

    /// No such process on this guest agent.
    #[error("no process with that identifier")]
    UnknownProcess,

    /// A process's input queue is full because the process is not reading it.
    #[error("the process is not reading its input fast enough")]
    InputBacklogFull,

    /// A process's input is closed: it closed it, or a write to it failed.
    #[error("the process's input is closed")]
    InputClosed,

    /// A process started without a handle the guest agent needs to run it: a
    /// pipe, or a process identifier it can signal.
    #[error("the process started without a handle the guest agent needs")]
    LaunchIncomplete,

    /// The tenant leaf every started process moves into cannot be entered:
    /// its descriptor was lost, or the leaf did not read as one.
    #[error("the tenant leaf cannot be entered")]
    TenantUnavailable {
        /// The operating system's reason.
        #[source]
        source: io::Error,
    },

    /// A freeze or thaw reached a guest agent that places no process in a
    /// tenant leaf: its processes run where it does, and it holds none.
    #[error("this guest agent has no tenant leaf to freeze")]
    NoTenantLeaf,

    /// The tenant leaf would not freeze or thaw; a failed freeze was thawed
    /// before it was answered.
    #[error("the tenant leaf could not be {state}")]
    TenantUnfrozen {
        /// The state it never reached: `frozen` or `thawed`.
        state: &'static str,
        /// The operating system's reason.
        #[source]
        source: io::Error,
    },

    /// The guest agent serves processes only: a stand-in that inherited the
    /// trait's file calls rather than serving them.
    #[error("this guest agent serves no file calls")]
    FileCallsUnsupported,

    /// A renewal or a hosts file reached a guest agent that builds no inner
    /// environment: its tenant processes run where it does.
    #[error("this guest agent builds no inner environment")]
    NoInnerEnvironment,

    /// The inner environment tenant processes run in could not be entered,
    /// torn down or built again.
    #[error("the inner environment is unavailable")]
    InnerUnavailable {
        /// The operating system's reason.
        #[source]
        source: io::Error,
    },

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
    pub const fn code(&self) -> ErrorCode {
        error_code::INTERNAL_OPERATION_FAILED
    }

    /// What anyone past this host is told: the failure and, when it has one,
    /// its cause, never the registry code or a backtrace, which are this
    /// host's. The other end of the socket reads it, and so does a model
    /// reading why its tool call failed.
    #[must_use]
    pub fn wire_message(&self) -> String {
        let kind = self.kind();
        std::error::Error::source(kind)
            .map_or_else(|| kind.to_string(), |cause| format!("{kind}: {cause}"))
    }

    /// Whether the path left the workspace, by name or through a link. True
    /// of the refusal where it is raised and of the answer the client reads,
    /// so a handler on either end tells it from every other failure.
    #[must_use]
    pub fn is_path_refused(&self) -> bool {
        self.rpc_code() == PATH_REFUSED_CODE
    }

    /// Whether a file call named something the workspace does not have, on
    /// either end of the socket.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        self.rpc_code() == FILE_NOT_FOUND_CODE
    }

    /// Whether a process call named a process the guest agent no longer holds,
    /// on either end of the socket: one that ended, or never was.
    #[must_use]
    pub fn is_unknown_process(&self) -> bool {
        self.rpc_code() == UNKNOWN_PROCESS_CODE
    }

    /// Whether a freeze or thaw reached a guest agent with no tenant leaf, on
    /// either end of the socket: it holds no tenant process, so none is
    /// frozen.
    #[must_use]
    pub fn is_no_tenant_leaf(&self) -> bool {
        self.rpc_code() == NO_TENANT_LEAF_CODE
    }

    /// Whether a process would not take what was written to it, on either
    /// end of the socket: it closed its input, or has not read what it was
    /// sent. The process runs on; the sandbox is not gone.
    #[must_use]
    pub fn is_input_refused(&self) -> bool {
        self.rpc_code() == CALL_EXECUTION_FAILED_CODE
    }

    /// The JSON-RPC code a refusal of this kind is answered with, or carries
    /// once the client decoded it: what both ends compare on.
    #[must_use]
    pub fn rpc_code(&self) -> i32 {
        match self.kind() {
            ErrorKind::Refused { code, .. } => *code,
            ErrorKind::PathRefused => PATH_REFUSED_CODE,
            ErrorKind::UnknownProcess => UNKNOWN_PROCESS_CODE,
            ErrorKind::NotFound { .. } => FILE_NOT_FOUND_CODE,
            ErrorKind::NoTenantLeaf => NO_TENANT_LEAF_CODE,
            ErrorKind::InputBacklogFull | ErrorKind::InputClosed => CALL_EXECUTION_FAILED_CODE,
            ErrorKind::InvalidParams { .. } | ErrorKind::Malformed { .. } | ErrorKind::NotAFile => {
                INVALID_PARAMS_CODE
            }
            ErrorKind::Io { source } if is_caller_mistake(source) => INVALID_PARAMS_CODE,
            ErrorKind::Io { .. }
            | ErrorKind::ConnectionLost
            | ErrorKind::Redial { .. }
            | ErrorKind::Unresponsive { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::LaunchIncomplete
            | ErrorKind::TenantUnavailable { .. }
            | ErrorKind::TenantUnfrozen { .. }
            | ErrorKind::FileCallsUnsupported
            | ErrorKind::NoInnerEnvironment
            | ErrorKind::InnerUnavailable { .. } => INTERNAL_ERROR_CODE,
        }
    }
}

/// Whether an operating-system refusal is about what the caller asked for —
/// a name that is not there, or is the wrong kind of thing — rather than
/// about the guest agent.
fn is_caller_mistake(failure: &io::Error) -> bool {
    matches!(
        failure.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::NotADirectory
            | io::ErrorKind::IsADirectory
            | io::ErrorKind::AlreadyExists
            | io::ErrorKind::DirectoryNotEmpty
            | io::ErrorKind::PermissionDenied
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::InvalidFilename
    )
}

/// The refusal the client decodes for a process the guest agent no longer
/// holds, as a stand-in guest agent in a sibling crate's suite answers a write.
#[must_use]
pub fn unknown_process_refused() -> Error {
    refused(UNKNOWN_PROCESS_CODE, &ErrorKind::UnknownProcess.to_string())
}

/// The refusal the client decodes for a process that closed its input, as a
/// stand-in guest agent answers a write.
#[must_use]
pub fn input_closed_refused() -> Error {
    refused(
        CALL_EXECUTION_FAILED_CODE,
        &ErrorKind::InputClosed.to_string(),
    )
}

/// The refusal the client decodes for a freeze or thaw sent to a guest agent
/// with no tenant leaf, as a stand-in guest agent answers one.
#[must_use]
pub fn no_tenant_leaf_refused() -> Error {
    refused(NO_TENANT_LEAF_CODE, &ErrorKind::NoTenantLeaf.to_string())
}
