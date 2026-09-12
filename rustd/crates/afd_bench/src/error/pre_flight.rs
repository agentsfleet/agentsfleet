//! Which refusals happen before anything is opened.
//!
//! Split from the variants at the file cap. The one method here is the one
//! the binaries read to say "nothing was created": these refusals happen
//! before the lane begins fixture work, and a reader deserves to be told there
//! is nothing to sweep rather than left wondering.

use super::Error;
impl Error {
    /// Whether this refusal happened before the lane created any fixture.
    ///
    /// Admission and shared-state refusals answer `true`; everything raised
    /// once fixture, file, or task work is in play answers `false`. Endpoint
    /// verification may already have opened a connection to inspect advertised
    /// topology, but it has not created benchmark state.
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
            | Self::RigIdentityUnavailable { .. }
            | Self::RigIdentityUnverified { .. }
            | Self::RigLockUnavailable { .. }
            | Self::RigAlreadyClaimed
            | Self::RigNotExclusive { .. }
            | Self::SharedTargetState { .. }
            | Self::VariableUnset { .. }
            | Self::VariableUnreadable { .. }
            | Self::SampleUnreadable { .. }
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
            | Self::FixtureIdentity(_)
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
