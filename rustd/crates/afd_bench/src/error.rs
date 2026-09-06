//! The one error type this crate returns, and why every variant is a refusal.
//!
//! # A bench failure is a refusal to measure, never a bad measurement
//!
//! Everything here is raised BEFORE or INSTEAD OF a number. A cap exceeded, a
//! missing acknowledgement, a datastore that will not answer — each one ends
//! the run with no result file written. That is deliberate: a partial run read
//! as a measurement is worse than no run at all, because it is a number
//! somebody will quote. The one thing this type never describes is a slow
//! result; slowness is the output, not a failure.

//! # No registry code, deliberately
//!
//! Twelve members already carry a plain `thiserror` type rather than
//! `afd_core::error_shell!`, and this crate joins them. The shell exists to
//! attach an operator-facing registry code to a failure an API will render;
//! nothing here is ever rendered to a tenant. Minting `BENCH_*` codes would
//! grow the registry an operator reads with entries only a developer running a
//! make target can ever see.

use crate::profile::Profile;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`docs/RUST_ERROR_STANDARD.md` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A refusal to run a measurement.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A profile name nothing maps to.
    ///
    /// Raised on the way in rather than defaulted, because defaulting an
    /// unrecognised profile would silently pick someone's blast radius.
    #[error("unknown profile {name:?}: expected one of {expected}")]
    UnknownProfile {
        /// What the caller asked for.
        name: String,
        /// The profiles that do exist, comma-separated.
        expected: &'static str,
    },

    /// A parameter above the active profile's ceiling.
    #[error(
        "{parameter} of {requested} exceeds the {profile} cap of {cap}: \
         lower it, or run the rig profile"
    )]
    CapExceeded {
        /// The profile whose ceiling was hit.
        profile: Profile,
        /// Which knob was turned too far.
        parameter: &'static str,
        /// What the caller asked for.
        requested: u64,
        /// The ceiling that refused it.
        cap: u64,
    },

    /// The production profile without its acknowledgement variable.
    ///
    /// Its own variant rather than a cap, because the answer is not "use a
    /// smaller number" — it is "say out loud that you meant production".
    #[error("profile prod refuses to run without {variable}={expected}")]
    AcknowledgementMissing {
        /// The variable that must be set.
        variable: &'static str,
        /// The exact value it must carry.
        expected: &'static str,
    },

    /// A deployed profile with nowhere to point.
    #[error(
        "profile {profile} needs a target: set {variable} to an address, \
         or to {rig} to run its semantics against the compose rig"
    )]
    TargetMissing {
        /// The profile that has no target.
        profile: Profile,
        /// The variable that would supply one.
        variable: &'static str,
        /// The value that selects the local rig.
        rig: &'static str,
    },
}

impl Error {
    /// Whether this refusal happened before anything was opened or created.
    ///
    /// Every variant currently answers `true`, and the method exists so that
    /// stays a decision rather than an accident: a future variant raised after
    /// a connection opens has to add its own arm, and the caller that reports
    /// "no fixture was created" will stop being able to say so for free.
    #[must_use]
    pub const fn is_pre_flight(&self) -> bool {
        match self {
            Self::UnknownProfile { .. }
            | Self::CapExceeded { .. }
            | Self::AcknowledgementMissing { .. }
            | Self::TargetMissing { .. } => true,
        }
    }
}
