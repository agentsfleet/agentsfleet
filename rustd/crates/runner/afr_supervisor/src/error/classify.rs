//! Failure classes retain the daemon refusal and its original code.

use super::{Error, ErrorKind, UNAUTHORIZED};
use crate::client::Verb;
use af_core::error_code::{self, ErrorCode};

impl Error {
    /// Whether retrying the same call could succeed.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self.kind() {
            ErrorKind::Stream { source } => matches!(
                source.reason(),
                af_stream::Reason::Network | af_stream::Reason::Timeout | af_stream::Reason::Closed
            ),
            ErrorKind::StreamRefused { retryable, .. } => *retryable,
            ErrorKind::Disconnected
            | ErrorKind::StreamDisconnected { .. }
            | ErrorKind::StreamTimeout { .. }
            | ErrorKind::Transport { .. }
            | ErrorKind::Unavailable { .. } => true,
            _ => false,
        }
    }

    /// The registry code the daemon refused with, when this is a refusal that
    /// named one.
    #[must_use]
    pub const fn refusal_code(&self) -> Option<ErrorCode> {
        match self.kind() {
            ErrorKind::StreamRefused { code, .. } => Some(*code),
            ErrorKind::Refused { code, .. } => *code,
            ErrorKind::Stream { .. }
            | ErrorKind::Disconnected
            | ErrorKind::StreamDisconnected { .. }
            | ErrorKind::StreamProtocol
            | ErrorKind::StreamProtocolSource { .. }
            | ErrorKind::StreamTimeout { .. }
            | ErrorKind::Io { .. }
            | ErrorKind::Crypto { .. }
            | ErrorKind::Config { .. }
            | ErrorKind::Boundary { .. }
            | ErrorKind::Address { .. }
            | ErrorKind::Client { .. }
            | ErrorKind::CheckpointTransfer { .. }
            | ErrorKind::Transport { .. }
            | ErrorKind::Unavailable { .. }
            | ErrorKind::TokenRefused
            | ErrorKind::Malformed { .. }
            | ErrorKind::Encode { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::Git { .. }
            | ErrorKind::LeaseSize { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::EgressUnresolved { .. }
            | ErrorKind::EgressNoIpv4 { .. }
            | ErrorKind::EgressBlocked { .. }
            | ErrorKind::Egress { .. } => None,
        }
    }

    /// The status the daemon refused with, when this is a refusal.
    #[must_use]
    pub const fn refusal_status(&self) -> Option<u16> {
        match self.kind() {
            ErrorKind::Refused { status, .. } => Some(*status),
            ErrorKind::StreamRefused { .. }
            | ErrorKind::Stream { .. }
            | ErrorKind::Disconnected
            | ErrorKind::StreamDisconnected { .. }
            | ErrorKind::StreamProtocol
            | ErrorKind::StreamProtocolSource { .. }
            | ErrorKind::StreamTimeout { .. }
            | ErrorKind::Io { .. }
            | ErrorKind::Crypto { .. }
            | ErrorKind::Config { .. }
            | ErrorKind::Boundary { .. }
            | ErrorKind::Address { .. }
            | ErrorKind::Client { .. }
            | ErrorKind::CheckpointTransfer { .. }
            | ErrorKind::Transport { .. }
            | ErrorKind::Unavailable { .. }
            | ErrorKind::TokenRefused
            | ErrorKind::Malformed { .. }
            | ErrorKind::Encode { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::Git { .. }
            | ErrorKind::LeaseSize { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::EgressUnresolved { .. }
            | ErrorKind::EgressNoIpv4 { .. }
            | ErrorKind::EgressBlocked { .. }
            | ErrorKind::Egress { .. } => None,
        }
    }

    /// Whether the daemon refused this runner's token.
    #[must_use]
    pub fn is_unauthorized(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::StreamRefused {
                code: error_code::RUN_INVALID_RUNNER_TOKEN,
                ..
            } | ErrorKind::Refused {
                status: UNAUTHORIZED,
                ..
            } | ErrorKind::TokenRefused
        )
    }

    /// Why a memory push that ended in this failure did not land, as the
    /// push-failure family labels it.
    #[must_use]
    pub const fn push_failure(&self) -> afr_telemetry::labels::PushFailure {
        use afr_telemetry::labels::PushFailure;
        match self.kind() {
            ErrorKind::Stream { .. }
            | ErrorKind::Disconnected
            | ErrorKind::StreamDisconnected { .. }
            | ErrorKind::Transport { .. }
            | ErrorKind::StreamTimeout { .. } => PushFailure::Transport,
            ErrorKind::Unavailable { .. } => PushFailure::Upstream,
            ErrorKind::StreamRefused { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::TokenRefused => PushFailure::Refused,
            ErrorKind::StreamProtocol
            | ErrorKind::StreamProtocolSource { .. }
            | ErrorKind::Io { .. }
            | ErrorKind::Crypto { .. }
            | ErrorKind::Config { .. }
            | ErrorKind::Boundary { .. }
            | ErrorKind::Address { .. }
            | ErrorKind::Client { .. }
            | ErrorKind::CheckpointTransfer { .. }
            | ErrorKind::Malformed { .. }
            | ErrorKind::Encode { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::Git { .. }
            | ErrorKind::LeaseSize { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::EgressUnresolved { .. }
            | ErrorKind::EgressNoIpv4 { .. }
            | ErrorKind::EgressBlocked { .. }
            | ErrorKind::Egress { .. } => PushFailure::Internal,
        }
    }

    /// Whether a host the fleet allowed resolved to an address no fleet may
    /// reach, which the fleet's owner fixes rather than the runner's.
    #[must_use]
    pub const fn is_egress_blocked(&self) -> bool {
        matches!(self.kind(), ErrorKind::EgressBlocked { .. })
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Stream { source } => source.code(),
            ErrorKind::StreamRefused { code, .. }
            | ErrorKind::Refused {
                code: Some(code), ..
            } => *code,
            ErrorKind::Disconnected
            | ErrorKind::StreamTimeout { .. }
            | ErrorKind::StreamDisconnected { .. } => error_code::RUN_STREAM_DISCONNECTED,
            ErrorKind::StreamProtocolSource { .. } | ErrorKind::StreamProtocol => {
                error_code::RUN_PROTOCOL_UNSUPPORTED
            }
            ErrorKind::Refused {
                status: UNAUTHORIZED,
                ..
            }
            | ErrorKind::TokenRefused => error_code::RUN_INVALID_RUNNER_TOKEN,
            ErrorKind::Refused {
                verb: Verb::Renew, ..
            } => error_code::RUN_LEASE_LOST,
            ErrorKind::Transport { verb, .. }
            | ErrorKind::Unavailable { verb, .. }
            | ErrorKind::Refused { verb, .. }
            | ErrorKind::Malformed { verb, .. } => verb.code(),
            ErrorKind::Egress { source } => source.code(),
            ErrorKind::Io { .. }
            | ErrorKind::Crypto { .. }
            | ErrorKind::Config { .. }
            | ErrorKind::Boundary { .. }
            | ErrorKind::Address { .. }
            | ErrorKind::Client { .. }
            | ErrorKind::CheckpointTransfer { .. }
            | ErrorKind::Encode { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::Git { .. }
            | ErrorKind::LeaseSize { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::EgressUnresolved { .. }
            | ErrorKind::EgressNoIpv4 { .. }
            | ErrorKind::EgressBlocked { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}
