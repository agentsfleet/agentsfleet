//! Failure classes retain the daemon refusal and its original code.

use super::{Error, ErrorKind, UNAUTHORIZED};
use crate::client::Verb;
use afd_core::error_code::{self, ErrorCode};

impl Error {
    /// Whether retrying the same call could succeed.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Transport { .. } | ErrorKind::Unavailable { .. }
        )
    }

    /// The registry code the daemon refused with, when this is a refusal that
    /// named one.
    #[must_use]
    pub const fn refusal_code(&self) -> Option<ErrorCode> {
        match self.kind() {
            ErrorKind::Refused { code, .. } => *code,
            _other => None,
        }
    }

    /// The status the daemon refused with, when this is a refusal.
    #[must_use]
    pub const fn refusal_status(&self) -> Option<u16> {
        match self.kind() {
            ErrorKind::Refused { status, .. } => Some(*status),
            _other => None,
        }
    }

    /// Whether the daemon refused this runner's token.
    #[must_use]
    pub const fn is_unauthorized(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Refused {
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
            ErrorKind::Unavailable { .. } => PushFailure::Upstream,
            ErrorKind::Refused { .. } | ErrorKind::TokenRefused => PushFailure::Refused,
            ErrorKind::Transport { .. } => PushFailure::Transport,
            _local => PushFailure::Internal,
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
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Refused {
                code: Some(code), ..
            } => *code,
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
            | ErrorKind::Persist { .. }
            | ErrorKind::Config { .. }
            | ErrorKind::Address { .. }
            | ErrorKind::Client { .. }
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
