//! The one error type this crate returns.
//!
//! Every variant refuses to write a
//! partial or misleading measurement. This developer-only harness uses plain
//! `thiserror`; no failure reaches a tenant or needs an operator registry code.

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
    /// A server counter the lane could not read a number out of.
    #[error("the {datastore} counter would not parse: no {field} field in the reply")]
    CounterUnreadable {
        /// Which datastore answered.
        datastore: &'static str,
        /// The field that was expected.
        field: &'static str,
    },
    /// A knob that was set to something this lane cannot read as a number.
    #[error("{variable}={value:?} is not a number this lane can use")]
    VariableUnreadable {
        /// The variable that was set.
        variable: &'static str,
        /// What it was set to.
        value: String,
    },
    /// A sample position was not an unsigned integer.
    #[error("benchmark sample {value:?} is not an unsigned integer")]
    SampleUnreadable {
        /// What the operator supplied.
        value: String,
        /// Why Rust could not parse it.
        #[source]
        source: std::num::ParseIntError,
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
    /// A task the lane spawned did not come back.
    ///
    /// No source: a join failure is a panic or a cancellation in this process,
    /// and the panic's own message has already been printed by the runtime.
    /// The role names which task, because a lost readiness sampler and a lost
    /// delivery worker are different failures to a reader.
    #[error("the {role} task was lost, so the window measured less than it drove")]
    TaskLost {
        /// What the task was doing.
        role: &'static str,
    },
    /// A parameter below the floor every run needs.
    ///
    /// The caps bound from above; this bounds from below. Zero runners spawn
    /// nothing and would write a rate of zero over the last real result.
    #[error("{parameter} of {requested} is below the floor of {floor}")]
    BelowFloor {
        /// Which knob.
        parameter: &'static str,
        /// What was asked for.
        requested: u64,
        /// The least the lane will run with.
        floor: u64,
    },
    /// A window shorter than the profile's warmup floor.
    #[error(
        "a window of {requested_ms} ms is under the {profile} warmup floor of {floor_ms} ms: a rate measured across a cold cache is not a rate"
    )]
    WindowTooShort {
        /// The profile whose floor refused it.
        profile: Profile,
        /// What was asked for.
        requested_ms: u128,
        /// The floor.
        floor_ms: u128,
    },
    /// More concurrent runners than the pool has connections.
    ///
    /// Refused rather than measured: above the pool size every poll's latency
    /// is sqlx acquire-wait inside the bench process, and the p95 would be a
    /// property of the harness reported under the datastore's name.
    #[error(
        "{runners} runners over a pool of {pool} connections would measure the pool, not Postgres: raise DATABASE_POOL_SIZE_API or lower BENCH_RUNNERS"
    )]
    RunnersExceedPool {
        /// Runners asked for.
        runners: u64,
        /// Connections the pool may open.
        pool: u32,
    },
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
    /// A deterministic fixture identifier could not be encoded.
    #[error("the benchmark fixture identifier would not encode")]
    FixtureIdentity(#[from] afd_core::error::Error),
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
    /// Two distributions that would not fold together.
    #[error("two latency distributions would not merge")]
    LatencyUnmergeable {
        /// What `HdrHistogram` refused.
        #[from]
        source: hdrhistogram::errors::AdditionError,
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
    /// A saturation run pointed outside the repository-owned loopback rig.
    #[error("{surface} points at {address:?}; the rig saturation profile accepts loopback only")]
    UnsafeTarget {
        /// Which configured or discovered address failed closed.
        surface: &'static str,
        /// Host only, with credentials deliberately excluded.
        address: String,
    },
    /// Docker could not establish which compose service owns a rig endpoint.
    #[error("the repository-owned compose rig identity could not be read for {service}")]
    RigIdentityUnavailable {
        /// Compose service being verified.
        service: &'static str,
        /// What starting Docker reported.
        #[source]
        source: std::io::Error,
    },
    /// A loopback endpoint is not the port published by this worktree's rig.
    #[error("{surface} is not owned by this worktree's compose {service} service")]
    RigIdentityUnverified {
        /// Configured endpoint being checked.
        surface: &'static str,
        /// Compose service that must own it.
        service: &'static str,
    },

    /// The deployment-wide outbound stream already contains another workload.
    #[error("the shared outbound stream contains {entries} existing entries")]
    SharedTargetState {
        /// Entries the lane did not create and cannot safely consume.
        entries: u64,
    },

    /// The operator interrupted a measurement before it completed.
    #[error("the benchmark was cancelled; its prefix sweep still ran")]
    Cancelled,

    /// The process could not install its cancellation listener.
    #[error("the benchmark cancellation listener failed")]
    InterruptUnavailable {
        /// What the operating system reported.
        #[source]
        source: std::io::Error,
    },

    /// Archived evidence is incomplete, inconsistent, or changed.
    #[error("datastore evidence is invalid: {0}")]
    EvidenceInvalid(String),

    /// A source-control or metadata command could not start.
    #[error("the {operation} evidence command would not start")]
    EvidenceCommand {
        /// Which source-control or metadata operation was starting.
        operation: &'static str,
        /// What the operating system reported.
        #[source]
        source: std::io::Error,
    },
}

mod pre_flight;
