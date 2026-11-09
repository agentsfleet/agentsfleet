//! The questions a caller may ask of a refusal, and the code each one answers.

use af_core::error_code::{self, ErrorCode};

use super::{Error, ErrorKind};

impl Error {
    /// The registry code this refusal answers.
    ///
    /// Reused rather than minted; the module note on [`crate::error`] says why.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::DatabaseUnavailable { .. }
            | ErrorKind::StatementsUnreadable { .. }
            | ErrorKind::SamplerUnavailable { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::FixtureUnseedable { .. } | ErrorKind::FixtureRefused { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            ErrorKind::QueueUnavailable { .. } | ErrorKind::BusUnavailable { .. } => {
                error_code::STARTUP_DRAGONFLY_CONNECT
            }
            _ if self.is_pre_flight() => error_code::STARTUP_ENV_CHECK,
            ErrorKind::ConcurrencyUnrepresentable { .. }
            | ErrorKind::UnknownProfile { .. }
            | ErrorKind::CapExceeded { .. }
            | ErrorKind::AcknowledgementMissing { .. }
            | ErrorKind::SteerPathFaulted { .. }
            | ErrorKind::LeasePathFaulted { .. }
            | ErrorKind::OutboundPathFaulted { .. }
            | ErrorKind::LedgerUnreadable { .. }
            | ErrorKind::CounterUnreadable { .. }
            | ErrorKind::VariableUnreadable { .. }
            | ErrorKind::UnknownLane { .. }
            | ErrorKind::VariableUnset { .. }
            | ErrorKind::TaskLost { .. }
            | ErrorKind::BelowFloor { .. }
            | ErrorKind::WindowTooShort { .. }
            | ErrorKind::RunnersExceedPool { .. }
            | ErrorKind::RunnerUnenrollable { .. }
            | ErrorKind::InstrumentUnavailable { .. }
            | ErrorKind::InstrumentUnflushable { .. }
            | ErrorKind::InstrumentPoisoned
            | ErrorKind::ResultUnrenderable { .. }
            | ErrorKind::ResultUnwritable { .. }
            | ErrorKind::ResultUnreadable { .. }
            | ErrorKind::ResultUnparseable { .. }
            | ErrorKind::LatencyUnavailable { .. }
            | ErrorKind::LatencyUnmergeable { .. }
            | ErrorKind::LatencyUnrecordable { .. }
            | ErrorKind::TargetMissing { .. }
            | ErrorKind::PlatformDefaultHeld { .. }
            | ErrorKind::CredentialUnsealable { .. }
            | ErrorKind::CredentialUnstorable { .. }
            | ErrorKind::CredentialMalformed { .. }
            | ErrorKind::IdentifierRefused { .. }
            | ErrorKind::CeilingRefused { .. }
            | ErrorKind::LeaseUnreadable { .. }
            | ErrorKind::UnknownShape { .. }
            | ErrorKind::ShapeNeedsRig { .. }
            | ErrorKind::AuthoringPathFaulted { .. }
            | ErrorKind::LiveSetFaulted { .. }
            | ErrorKind::NothingIssued { .. }
            | ErrorKind::InvariantBroken { .. }
            | ErrorKind::DrainIncomplete { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}
