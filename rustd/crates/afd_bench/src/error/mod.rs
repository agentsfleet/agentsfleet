//! The one error type this crate returns, and why every kind is a refusal.
//!
//! # A bench failure is a refusal to measure, never a bad measurement
//!
//! Everything here is raised BEFORE or INSTEAD OF a number. A cap exceeded, a
//! missing acknowledgement, a datastore that will not answer — each one ends
//! the run with no result file written. That is deliberate: a partial run read
//! as a measurement is worse than no run at all, because it is a number
//! somebody will quote. The one thing this type never describes is a slow
//! result; slowness is the output, not a failure.
//!
//! # The shared hull, like every crate whose error carries a cause
//!
//! Most kinds wrap the failure beneath them (`afd_db`, `sqlx`, the
//! filesystem) or carry the numbers that refused a run, which is the property
//! `docs/RUST_ERROR_STANDARD.md` §"The shared hull" uses to put a crate on the
//! hull. So [`Error`] is the `afd_core::error_shell!` struct over a private
//! `ErrorKind`, and callers ask `is_*` questions instead of
//! matching variants.
//!
//! The codes come from the existing registry rather than a `BENCH_*` family:
//! nothing here reaches a tenant, and minting codes only a developer running a
//! make target can see would grow the list an operator reads. A refusal from
//! the environment answers the startup environment check; one from a datastore
//! answers that datastore's code; the rest answer the internal operation code.

mod accessors;
mod kind;
mod pre_flight;

pub(crate) use kind::ErrorKind;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`docs/RUST_ERROR_STANDARD.md` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A refusal to run a measurement, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => DatabaseUnavailable,
    afd_dragonfly::Error => QueueUnavailable,
    afd_events::Error => SteerPathFaulted,
    afd_fleet::Error => LeasePathFaulted,
    afd_admission::Error => LedgerUnreadable,
    afd_runner::Error => RunnerUnenrollable,
    sqlx::Error => FixtureUnseedable,
    afd_observability::Error => InstrumentUnavailable,
    opentelemetry_sdk::error::OTelSdkError => InstrumentUnflushable,
    serde_json::Error => ResultUnrenderable,
    hdrhistogram::CreationError => LatencyUnavailable,
    hdrhistogram::errors::AdditionError => LatencyUnmergeable,
    hdrhistogram::RecordError => LatencyUnrecordable,
    afd_crypto::error::Error => CredentialUnsealable,
);
