//! What actually refused a run. Private, so a new kind is not a breaking change.
//!
//! The lane's own machinery — the statement counter, the credential the drain
//! seals, the lease answers it reads back, the stream ceiling — shares this
//! enum with the path under measurement: they are refusals of the same run.

use std::path::PathBuf;

use crate::profile::Profile;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// Raised on the way in rather than defaulted, because defaulting an
    /// unrecognised profile would silently pick someone's blast radius.
    #[error("unknown profile {name:?}: expected one of {expected}")]
    UnknownProfile {
        name: String,
        expected: &'static str,
    },

    #[error(
        "{parameter} of {requested} exceeds the {profile} cap of {cap}: \
         lower it, or run the rig profile"
    )]
    CapExceeded {
        profile: Profile,
        parameter: &'static str,
        requested: u64,
        cap: u64,
    },

    /// Its own kind rather than a cap, because the answer is not "use a
    /// smaller number" — it is "say out loud that you meant production".
    #[error("profile prod refuses to run without {variable}={expected}")]
    AcknowledgementMissing {
        variable: &'static str,
        expected: &'static str,
    },

    #[error("the bench database would not open")]
    DatabaseUnavailable { source: afd_db::Error },

    #[error("the bench queue would not open")]
    QueueUnavailable { source: afd_dragonfly::Error },

    #[error("the steer path would not run")]
    SteerPathFaulted { source: afd_events::Error },

    #[error("the lease path would not run")]
    LeasePathFaulted { source: afd_fleet::Error },

    #[error("the admission ledger would not answer")]
    LedgerUnreadable { source: afd_admission::Error },

    /// Raised rather than summed to zero: a datastore that answered its
    /// statistics command with nothing this crate recognises did not serve
    /// zero operations, and the attribution would be a zero nobody measured.
    #[error("the {datastore} counter would not parse: no {field} field in the reply")]
    CounterUnreadable {
        datastore: &'static str,
        field: &'static str,
    },

    /// Refused rather than defaulted: `BENCH_FLEETS=1O00` with a letter O
    /// would otherwise measure the default population under the name of the
    /// one asked for.
    #[error("{variable}={value:?} is not a number this lane can use")]
    VariableUnreadable {
        variable: &'static str,
        value: String,
    },

    #[error("unknown lane: {usage}")]
    UnknownLane { usage: &'static str },

    /// Named rather than defaulted: guessing a datastore URL is how a bench
    /// run lands somewhere nobody chose.
    #[error("{variable} is unset, and this lane will not guess one")]
    VariableUnset { variable: &'static str },

    /// No source: a join failure is a panic or a cancellation in this process,
    /// and the panic's own message has already been printed by the runtime.
    /// The role names which task, because a lost readiness sampler and a lost
    /// delivery worker are different failures to a reader.
    #[error("the {role} task was lost, so the window measured less than it drove")]
    TaskLost { role: &'static str },

    /// The caps bound from above; this bounds from below. Zero runners spawn
    /// nothing and would write a rate of zero over the last real result.
    #[error("{parameter} of {requested} is below the floor of {floor}")]
    BelowFloor {
        parameter: &'static str,
        requested: u64,
        floor: u64,
    },

    #[error(
        "a window of {requested_ms} ms is under the {profile} warmup floor of {floor_ms} ms: a rate measured across a cold cache is not a rate"
    )]
    WindowTooShort {
        profile: Profile,
        requested_ms: u128,
        floor_ms: u128,
    },

    /// Refused rather than measured: above the pool size every poll's latency
    /// is sqlx acquire-wait inside the bench process, and the p95 would be a
    /// property of the harness reported under the datastore's name.
    #[error(
        "{runners} runners over a pool of {pool} connections would measure the pool, not Postgres: raise DATABASE_POOL_SIZE_API or lower BENCH_RUNNERS"
    )]
    RunnersExceedPool { runners: u64, pool: u32 },

    #[error("the bench runner would not enrol")]
    RunnerUnenrollable { source: afd_runner::Error },

    #[error("the bench fixture would not seed")]
    FixtureUnseedable { source: sqlx::Error },

    /// Carries the observability crate's own error rather than its message:
    /// the census names the row it rejected, and stringifying here would drop
    /// that from the chain a reader walks.
    #[error("the lease instrument would not install")]
    InstrumentUnavailable { source: afd_observability::Error },

    #[error("the lease counters would not be collected")]
    InstrumentUnflushable {
        source: opentelemetry_sdk::error::OTelSdkError,
    },

    /// No source: a poisoned lock is a fact about this process, not a failure
    /// something else reported, and inventing a cause for it would be the
    /// chain-padding `docs/RUST_ERROR_STANDARD.md` rule 4 warns against.
    #[error("the captured lease counters are unreadable: a holder panicked")]
    InstrumentPoisoned,

    /// No path, because nothing was written: rendering happens before the
    /// temporary file is opened, so a failure here leaves the result path
    /// exactly as it was.
    #[error("the result would not render")]
    ResultUnrenderable { source: serde_json::Error },

    #[error("the result would not be written to {path}")]
    ResultUnwritable {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{path} would not be read")]
    ResultUnreadable {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Raised rather than skipped: a truncated result is the one thing a
    /// comparison must refuse, because reading it as an empty run would report
    /// a delta against numbers that were never measured.
    #[error("{path} is not a readable result")]
    ResultUnparseable {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("the latency histogram would not be created")]
    LatencyUnavailable { source: hdrhistogram::CreationError },

    #[error("two latency distributions would not merge")]
    LatencyUnmergeable {
        source: hdrhistogram::errors::AdditionError,
    },

    #[error("a measured latency would not be recorded")]
    LatencyUnrecordable { source: hdrhistogram::RecordError },

    #[error(
        "profile {profile} needs a target: set {variable} to an address, \
         or to {rig} to run its semantics against the compose rig"
    )]
    TargetMissing {
        profile: Profile,
        variable: &'static str,
        rig: &'static str,
    },

    /// Almost always a Postgres started without `pg_stat_statements` in
    /// `shared_preload_libraries`: the extension's view refuses to read
    /// until the library is loaded, and that happens only at server start.
    #[error(
        "the statement counter would not answer: is pg_stat_statements preloaded? \
         the compose postgres preloads it, so recreate it with make _ensure-test-infra"
    )]
    StatementsUnreadable { source: sqlx::Error },

    /// Refused rather than overwritten: the row is deployment-wide, and
    /// repointing it would change what every other fleet on this database
    /// resolves to.
    #[error(
        "a platform default for {provider} already exists on this database: \
         reset the rig (make _reset-test-db) before draining"
    )]
    PlatformDefaultHeld { provider: &'static str },

    #[error("the drain's platform credential would not seal")]
    CredentialUnsealable { source: afd_crypto::error::Error },

    /// The ceiling is sized for the widest rung and every rung joins its
    /// streams before the next opens, so a refusal means a stream outlived its
    /// rung. Refused rather than skipped: the rung divides its heap by the
    /// streams it opened, and a skipped one would understate every figure.
    #[error("the stream ceiling refused one of {opened} streams it was sized to hold")]
    CeilingRefused { opened: u64 },

    #[error("a lease answer would not parse as a lease response")]
    LeaseUnreadable { source: serde_json::Error },
}
