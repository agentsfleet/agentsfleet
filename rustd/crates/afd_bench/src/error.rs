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

use std::path::PathBuf;

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

    /// Postgres would not answer, or its URL would not resolve.
    #[error("the bench database would not open")]
    DatabaseUnavailable {
        /// What `afd_db` refused, naming the knob or the connection.
        #[from]
        source: afd_db::Error,
    },

    /// Redis would not answer.
    #[error("the bench queue would not open")]
    QueueUnavailable {
        /// What `afd_redis` refused.
        #[from]
        source: afd_redis::Error,
    },

    /// The steer ingress path faulted, which is not a measurement.
    #[error("the steer path would not run")]
    SteerPathFaulted {
        /// What `afd_events` reported.
        #[from]
        source: afd_events::Error,
    },

    /// The lease path itself faulted, which is not a measurement.
    #[error("the lease path would not run")]
    LeasePathFaulted {
        /// What `afd_fleet` reported.
        #[from]
        source: afd_fleet::Error,
    },

    /// A lane name nothing maps to.
    #[error("unknown lane: {usage}")]
    UnknownLane {
        /// How the arguments are spelled.
        usage: &'static str,
    },

    /// A variable the lane cannot proceed without.
    ///
    /// Named rather than defaulted: guessing a datastore URL is how a bench
    /// run lands somewhere nobody chose.
    #[error("{variable} is unset, and this lane will not guess one")]
    VariableUnset {
        /// The variable that must be set.
        variable: &'static str,
    },

    /// A runner's polling task did not come back.
    ///
    /// No source: a join failure is a panic or a cancellation in this process,
    /// and the panic's own message has already been printed by the runtime.
    #[error("a runner's polling task was lost, so the window measured less than it drove")]
    RunnerTaskLost,

    /// A runner would not enrol.
    #[error("the bench runner would not enrol")]
    RunnerUnenrollable {
        /// What `afd_runner` refused.
        #[from]
        source: afd_runner::Error,
    },

    /// A seeding statement would not land.
    #[error("the bench fixture would not seed")]
    FixtureUnseedable {
        /// What Postgres said.
        #[from]
        source: sqlx::Error,
    },

    /// The daemon's instrument set would not install.
    ///
    /// Carries the observability crate's own error rather than its message:
    /// the census names the row it rejected, and stringifying here would drop
    /// that from the chain a reader walks.
    #[error("the lease instrument would not install")]
    InstrumentUnavailable {
        /// What the census or the instrument set refused.
        #[from]
        source: afd_observability::Error,
    },

    /// The provider would not flush, so the counters cannot be trusted.
    #[error("the lease counters would not be collected")]
    InstrumentUnflushable {
        /// What the SDK said.
        #[from]
        source: opentelemetry_sdk::error::OTelSdkError,
    },

    /// A thread died holding the capture lock.
    ///
    /// No source: a poisoned lock is a fact about this process, not a failure
    /// something else reported, and inventing a cause for it would be the
    /// chain-padding `docs/RUST_ERROR_STANDARD.md` rule 4 warns against.
    #[error("the captured lease counters are unreadable: a holder panicked")]
    InstrumentPoisoned,

    /// A report that would not render to JSON.
    ///
    /// No path, because nothing was written: rendering happens before the
    /// temporary file is opened, so a failure here leaves the result path
    /// exactly as it was.
    #[error("the result would not render")]
    ResultUnrenderable {
        /// What serde refused.
        source: serde_json::Error,
    },

    /// A result file that would not land.
    #[error("the result would not be written to {path}")]
    ResultUnwritable {
        /// The path being written when it failed.
        path: PathBuf,
        /// What the filesystem said.
        source: std::io::Error,
    },

    /// A result or baseline file that would not open.
    #[error("{path} would not be read")]
    ResultUnreadable {
        /// The file that would not open.
        path: PathBuf,
        /// What the filesystem said.
        source: std::io::Error,
    },

    /// A file that is not a report.
    ///
    /// Raised rather than skipped: a truncated result is the one thing a
    /// comparison must refuse, because reading it as an empty run would report
    /// a delta against numbers that were never measured.
    #[error("{path} is not a readable result")]
    ResultUnparseable {
        /// The file that would not parse.
        path: PathBuf,
        /// Where serde gave up.
        source: serde_json::Error,
    },

    /// The latency histogram could not be built.
    #[error("the latency histogram would not be created")]
    LatencyUnavailable {
        /// What `HdrHistogram` refused, and why.
        #[from]
        source: hdrhistogram::CreationError,
    },

    /// A measured duration the histogram would not hold.
    #[error("a measured latency would not be recorded")]
    LatencyUnrecordable {
        /// The value and the ceiling that refused it.
        #[from]
        source: hdrhistogram::RecordError,
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
        // One arm per answer rather than one per family: clippy is right that
        // grouping by cause and then giving two groups the same body is a
        // distinction the code does not make. What decides this is whether a
        // connection was open when the failure was raised.
        match self {
            Self::UnknownProfile { .. }
            | Self::CapExceeded { .. }
            | Self::AcknowledgementMissing { .. }
            | Self::TargetMissing { .. }
            | Self::VariableUnset { .. }
            | Self::UnknownLane { .. } => true,
            Self::LatencyUnavailable { .. }
            | Self::LatencyUnrecordable { .. }
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
            | Self::RunnerTaskLost => false,
        }
    }
}
