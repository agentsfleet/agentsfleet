//! Which refusals happen before anything is opened.
//!
//! Split from the variants at the file cap. The one method here is the one
//! the binaries read to say "nothing was created": a refusal raised from the
//! environment and the parameters leaves nothing to sweep, and a reader
//! deserves to be told so rather than left wondering.

use super::Error;
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
        match self {
            Self::UnknownProfile { .. }
            | Self::CapExceeded { .. }
            | Self::AcknowledgementMissing { .. }
            | Self::TargetMissing { .. }
            | Self::UnsafeTarget { .. }
            | Self::SharedTargetState { .. }
            | Self::VariableUnset { .. }
            | Self::VariableUnreadable { .. }
            | Self::UnknownLane { .. }
            | Self::BelowFloor { .. }
            | Self::WindowTooShort { .. }
            | Self::RunnersExceedPool { .. } => true,
            Self::LatencyUnavailable { .. }
            | Self::LatencyUnrecordable { .. }
            | Self::LatencyUnmergeable { .. }
            | Self::ResultUnrenderable { .. }
            | Self::ResultUnwritable { .. }
            | Self::ResultUnreadable { .. }
            | Self::ResultUnparseable { .. }
            | Self::InstrumentUnavailable { .. }
            | Self::InstrumentUnflushable { .. }
            | Self::InstrumentPoisoned
            | Self::DatabaseUnavailable { .. }
            | Self::QueueUnavailable { .. }
            | Self::LeasePathFaulted { .. }
            | Self::SteerPathFaulted { .. }
            | Self::FixtureUnseedable { .. }
            | Self::RunnerUnenrollable { .. }
            | Self::TaskLost { .. }
            | Self::CounterUnreadable { .. }
            | Self::EvidenceInvalid(_)
            | Self::EvidenceCommand { .. }
            | Self::Cancelled
            | Self::InterruptUnavailable { .. } => false,
        }
    }
}
