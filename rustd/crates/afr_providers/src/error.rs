//! What a provider fails with.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull. A provider failure ends the run, and how the
//! report names it depends on which failure it was: a refusal no retry changes
//! is the fleet's error, with the status in its detail; a connection lost or a
//! turn the provider ended early is a transport loss.
//!
//! # Which codes, and why none is new
//!
//! An operator reads a provider failure on the host's journal, so it reuses the
//! registry's internal code as the rest of the runner does; the log line's
//! `event` says which failure it was. A policy naming a provider this runner
//! does not speak is the fleet's configuration, and takes that code.

use std::fmt;

use afd_core::error_code::{self, ErrorCode};
use afd_wire::report::FailureClass;
use rig_core::ProviderError;
use rig_core::message::EmptyToolName;

use crate::transport::Oversize;

pub(crate) mod raise;

afd_core::error_shell!(
    /// A provider failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way a provider fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The provider refused the turn with a status no retry changes.
    #[error("the model provider refused the turn with status {status}{}", Named(.code.as_deref()))]
    Refused {
        /// The HTTP status.
        status: u16,
        /// The provider's own name for the refusal, such as
        /// `context_length_exceeded`, never its message.
        code: Option<String>,
    },

    /// The connection was lost before the turn ended.
    #[error("the connection to the model provider was lost mid-turn")]
    Lost {
        /// The transport's reason.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The provider ended the turn early with an error of its own.
    #[error("the model provider ended the turn early: {reason}")]
    Ended {
        /// The provider's own name for the error, never its message.
        reason: String,
    },

    /// The provider's reply passed the bytes one turn may carry; the read
    /// ended there.
    #[error(transparent)]
    Oversize(Oversize),

    /// A request or a streamed event was not the JSON its wire defines.
    #[error("the model provider's turn could not be written or read")]
    Unreadable {
        /// The parser's reason.
        #[from]
        source: serde_json::Error,
    },

    /// The wire library could not build the turn's request, or read its
    /// reply as the wire defines it.
    #[error("the model provider's turn could not be built or read")]
    Wire {
        /// The library's reason.
        #[source]
        source: ProviderError,
    },

    /// The policy names a provider this runner does not speak.
    #[error("the policy names a model provider this runner does not speak: {provider}")]
    Unhosted {
        /// The provider, as the policy spells it.
        provider: String,
    },

    /// The conversation holds a result that answers no call before it.
    #[error("the conversation cannot be sent: the result for call {call_id} answers no call")]
    Unsendable {
        /// The id the result names.
        call_id: String,
    },

    /// The conversation holds a call with no tool name.
    #[error("the conversation cannot be sent: call {call_id} names no tool")]
    Unnamed {
        /// The call's id.
        call_id: String,
        /// rig's reason.
        #[source]
        source: EmptyToolName,
    },

    /// The provider registry names an entry whose base URL does not parse.
    #[error("the provider registry's entry {name} has a base URL that does not parse")]
    Registry {
        /// The entry's name.
        name: String,
        /// The parser's reason.
        #[source]
        source: url::ParseError,
    },

    /// The HTTP client could not be built.
    #[error("the model providers' HTTP client could not be built")]
    Client {
        /// The client's reason. Lifted by name, never by `?`: a send that
        /// fails is a lost connection, not a client that could not be built.
        #[source]
        source: reqwest::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// A refusal with `status`, which no retry changes.
    #[must_use]
    pub fn refused(status: u16) -> Self {
        Self::from(ErrorKind::Refused { status, code: None })
    }

    /// A connection lost mid-turn, for `source`.
    #[must_use]
    pub fn lost(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::from(ErrorKind::Lost {
            source: source.into(),
        })
    }

    /// The failure in a sentence, without its code or backtrace, for a
    /// report's detail.
    #[must_use]
    pub fn detail(&self) -> String {
        self.kind().to_string()
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Unhosted { .. } => error_code::AGENTSFLEET_INVALID_CONFIG,
            ErrorKind::Refused { .. }
            | ErrorKind::Lost { .. }
            | ErrorKind::Ended { .. }
            | ErrorKind::Oversize(_)
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Registry { .. }
            | ErrorKind::Unsendable { .. }
            | ErrorKind::Unnamed { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Client { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The class the report names, where the failure has one. A refusal, an
    /// unreadable or oversized turn and a provider this runner does not speak
    /// are the fleet's error and carry none.
    #[must_use]
    pub fn failure_class(&self) -> Option<FailureClass> {
        match self.kind() {
            ErrorKind::Lost { .. } | ErrorKind::Ended { .. } => Some(FailureClass::TransportLoss),
            ErrorKind::Refused { .. }
            | ErrorKind::Oversize(_)
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Unhosted { .. }
            | ErrorKind::Registry { .. }
            | ErrorKind::Unsendable { .. }
            | ErrorKind::Unnamed { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Client { .. } => None,
        }
    }

    /// The provider a refused lease named, for its log line.
    #[must_use]
    pub fn unhosted_provider(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Unhosted { provider } => Some(provider),
            ErrorKind::Refused { .. }
            | ErrorKind::Lost { .. }
            | ErrorKind::Ended { .. }
            | ErrorKind::Oversize(_)
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Registry { .. }
            | ErrorKind::Unsendable { .. }
            | ErrorKind::Unnamed { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Client { .. } => None,
        }
    }
}

/// A refusal's code in parentheses after its status, when the provider named
/// one: `status 400 (context_length_exceeded)`.
struct Named<'a>(Option<&'a str>);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(code) => write!(f, " ({code})"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
