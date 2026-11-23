//! What a provider fails with.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A provider failure ends the run, and how the
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
//!
//! Saved settings a validation refuses are the caller's request, and take the
//! request code where the daemon proves what an admin saves. The runner meets
//! the same refusal as the fleet's configuration: it keeps it as the cause of
//! a configuration failure, which takes that code.

use af_core::error_code::{self, ErrorCode};
use af_wire::report::FailureClass;

mod kind;
pub(crate) mod raise;

pub(crate) use self::kind::ErrorKind;

af_core::error_shell!(
    /// A provider failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// HTTP status from a physical decision refusal; the logical caller owns
    /// retry eligibility and reserves every permitted attempt independently.
    #[must_use]
    pub const fn decision_status(&self) -> Option<u16> {
        match self.kind() {
            ErrorKind::DecisionStatus { status, .. } => Some(*status),
            ErrorKind::DecisionEndpoint { .. }
            | ErrorKind::DecisionPreflight { .. }
            | ErrorKind::DecisionBuild { .. }
            | ErrorKind::DecisionOversize
            | ErrorKind::Refused { .. }
            | ErrorKind::Lost { .. }
            | ErrorKind::Ended { .. }
            | ErrorKind::Silent { .. }
            | ErrorKind::Oversize { .. }
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Unhosted { .. }
            | ErrorKind::BlockedEndpoint { .. }
            | ErrorKind::Conversation { .. }
            | ErrorKind::Configuration { .. }
            | ErrorKind::Route { .. }
            | ErrorKind::Bounds { .. }
            | ErrorKind::Url { .. }
            | ErrorKind::Endpoint
            | ErrorKind::DecisionCapabilities
            | ErrorKind::Unroutable { .. }
            | ErrorKind::Client { .. } => None,
        }
    }

    /// Provider-requested wait for a decision refusal. The logical caller still
    /// enforces its retry count, remaining time and fresh physical reservation.
    #[must_use]
    pub const fn decision_retry_after(&self) -> Option<std::time::Duration> {
        match self.kind() {
            ErrorKind::DecisionStatus {
                status: 429 | 529,
                retry_after,
            } => *retry_after,
            ErrorKind::DecisionEndpoint { .. }
            | ErrorKind::DecisionPreflight { .. }
            | ErrorKind::DecisionBuild { .. }
            | ErrorKind::DecisionOversize
            | ErrorKind::DecisionStatus { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::Lost { .. }
            | ErrorKind::Ended { .. }
            | ErrorKind::Silent { .. }
            | ErrorKind::Oversize { .. }
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Unhosted { .. }
            | ErrorKind::BlockedEndpoint { .. }
            | ErrorKind::Conversation { .. }
            | ErrorKind::Configuration { .. }
            | ErrorKind::Route { .. }
            | ErrorKind::Bounds { .. }
            | ErrorKind::Url { .. }
            | ErrorKind::Endpoint
            | ErrorKind::DecisionCapabilities
            | ErrorKind::Unroutable { .. }
            | ErrorKind::Client { .. } => None,
        }
    }

    /// Whether this decision failed before a physical request could be sent.
    #[must_use]
    pub const fn decision_not_sent(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::DecisionEndpoint { .. }
                | ErrorKind::DecisionBuild { .. }
                | ErrorKind::DecisionPreflight { .. }
                | ErrorKind::Configuration { .. }
                | ErrorKind::Route { .. }
                | ErrorKind::Bounds { .. }
                | ErrorKind::Url { .. }
                | ErrorKind::Endpoint
                | ErrorKind::DecisionCapabilities
                | ErrorKind::Unroutable { .. }
        )
    }

    /// A refusal with `status`, which no retry changes.
    #[must_use]
    pub fn refused(status: u16) -> Self {
        Self::from(ErrorKind::Refused {
            status,
            code: None,
            source: None,
        })
    }

    /// A connection lost mid-turn, for `source`.
    #[must_use]
    pub fn lost(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::from(ErrorKind::Lost {
            source: source.into(),
        })
    }

    /// A turn that ended with neither text nor a tool call; `cut` when the
    /// provider stopped it early rather than finishing it.
    #[must_use]
    pub fn silent(cut: bool) -> Self {
        Self::from(ErrorKind::Silent { cut })
    }

    /// The failure in a sentence, without its code or backtrace, for a
    /// report's detail.
    #[must_use]
    pub fn detail(&self) -> String {
        self.kind().to_string()
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::DecisionEndpoint { .. }
            | ErrorKind::DecisionBuild { .. }
            | ErrorKind::DecisionPreflight { .. }
            | ErrorKind::Unhosted { .. }
            | ErrorKind::BlockedEndpoint { .. }
            | ErrorKind::Configuration { .. }
            | ErrorKind::Route { .. } => error_code::AGENTSFLEET_INVALID_CONFIG,
            ErrorKind::Bounds { .. }
            | ErrorKind::Url { .. }
            | ErrorKind::Endpoint
            | ErrorKind::DecisionCapabilities
            | ErrorKind::Unroutable { .. } => error_code::INVALID_REQUEST,
            ErrorKind::DecisionOversize
            | ErrorKind::DecisionStatus { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::Lost { .. }
            | ErrorKind::Ended { .. }
            | ErrorKind::Silent { .. }
            | ErrorKind::Oversize { .. }
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Conversation { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Client { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The class the report names, where the failure has one. A refusal, an
    /// unreadable or oversized turn and a provider this runner does not speak
    /// are the fleet's error and carry none.
    #[must_use]
    pub const fn failure_class(&self) -> Option<FailureClass> {
        match self.kind() {
            ErrorKind::Lost { .. } | ErrorKind::Ended { .. } => Some(FailureClass::TransportLoss),
            ErrorKind::DecisionEndpoint { .. }
            | ErrorKind::DecisionBuild { .. }
            | ErrorKind::DecisionPreflight { .. }
            | ErrorKind::DecisionOversize
            | ErrorKind::DecisionStatus { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::Silent { .. }
            | ErrorKind::Oversize { .. }
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Unhosted { .. }
            | ErrorKind::BlockedEndpoint { .. }
            | ErrorKind::Configuration { .. }
            | ErrorKind::Route { .. }
            | ErrorKind::Conversation { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Bounds { .. }
            | ErrorKind::Url { .. }
            | ErrorKind::Endpoint
            | ErrorKind::DecisionCapabilities
            | ErrorKind::Unroutable { .. }
            | ErrorKind::Client { .. } => None,
        }
    }

    /// The provider a refused run named, for its log line.
    #[must_use]
    pub fn unhosted_provider(&self) -> Option<&str> {
        match self.kind() {
            ErrorKind::Unhosted { provider } => Some(provider),
            ErrorKind::DecisionEndpoint { .. }
            | ErrorKind::DecisionBuild { .. }
            | ErrorKind::DecisionPreflight { .. }
            | ErrorKind::DecisionOversize
            | ErrorKind::DecisionStatus { .. }
            | ErrorKind::BlockedEndpoint { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::Lost { .. }
            | ErrorKind::Ended { .. }
            | ErrorKind::Silent { .. }
            | ErrorKind::Oversize { .. }
            | ErrorKind::Unreadable { .. }
            | ErrorKind::Configuration { .. }
            | ErrorKind::Route { .. }
            | ErrorKind::Conversation { .. }
            | ErrorKind::Wire { .. }
            | ErrorKind::Bounds { .. }
            | ErrorKind::Url { .. }
            | ErrorKind::Endpoint
            | ErrorKind::DecisionCapabilities
            | ErrorKind::Unroutable { .. }
            | ErrorKind::Client { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests;
