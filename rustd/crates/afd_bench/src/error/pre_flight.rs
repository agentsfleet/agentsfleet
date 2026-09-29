//! Which refusals happen before anything is opened.
//!
//! Split from the variants at the file cap. The one method here is the one
//! the binaries read to say "nothing was created": a refusal raised from the
//! environment and the parameters leaves nothing to sweep, and a reader
//! deserves to be told so rather than left wondering.

use super::{Error, ErrorKind};
impl Error {
    /// Whether this refusal happened before anything was opened or created.
    ///
    /// The refusals a lane raises from the environment and its parameters
    /// answer `true`; everything raised once a datastore, a file or a task is
    /// in play answers `false`. The binaries read it to say "nothing was
    /// created" on a pre-flight refusal, so a reader knows there is nothing
    /// to sweep.
    #[must_use]
    pub const fn is_pre_flight(&self) -> bool {
        // One arm per answer rather than one per family: clippy is right that
        // grouping by cause and then giving two groups the same body is a
        // distinction the code does not make. What decides this is whether a
        // connection was open when the failure was raised.
        match self.kind() {
            ErrorKind::UnknownProfile { .. }
            | ErrorKind::CapExceeded { .. }
            | ErrorKind::AcknowledgementMissing { .. }
            | ErrorKind::TargetMissing { .. }
            | ErrorKind::VariableUnset { .. }
            | ErrorKind::VariableUnreadable { .. }
            | ErrorKind::UnknownLane { .. }
            | ErrorKind::BelowFloor { .. }
            | ErrorKind::WindowTooShort { .. }
            | ErrorKind::RunnersExceedPool { .. } => true,
            ErrorKind::LatencyUnavailable { .. }
            | ErrorKind::LatencyUnrecordable { .. }
            | ErrorKind::LatencyUnmergeable { .. }
            | ErrorKind::ResultUnrenderable { .. }
            | ErrorKind::ResultUnwritable { .. }
            | ErrorKind::ResultUnreadable { .. }
            | ErrorKind::ResultUnparseable { .. }
            | ErrorKind::InstrumentUnavailable { .. }
            | ErrorKind::InstrumentUnflushable { .. }
            | ErrorKind::InstrumentPoisoned
            | ErrorKind::DatabaseUnavailable { .. }
            | ErrorKind::QueueUnavailable { .. }
            | ErrorKind::LeasePathFaulted { .. }
            | ErrorKind::SteerPathFaulted { .. }
            | ErrorKind::LedgerUnreadable { .. }
            | ErrorKind::FixtureUnseedable { .. }
            | ErrorKind::RunnerUnenrollable { .. }
            | ErrorKind::TaskLost { .. }
            | ErrorKind::CounterUnreadable { .. }
            | ErrorKind::StatementsUnreadable { .. }
            | ErrorKind::PlatformDefaultHeld { .. }
            | ErrorKind::CredentialUnsealable { .. }
            | ErrorKind::CeilingRefused { .. }
            | ErrorKind::LeaseUnreadable { .. } => false,
        }
    }
}
