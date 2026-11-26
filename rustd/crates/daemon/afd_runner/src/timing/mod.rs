//! The daemon's run clock: how long a run is granted, how often a runner rounds, and how long
//! silence is tolerated.
//!
//! Each is a key of the daemon's `[runner.*]` config section, whose file shape is
//! `agentsfleetd`'s. Their defaults and bounds are declared here, beside the [`RunSettings`]
//! that checks them at boot and the [`RunClock`] every store reads. The spans both ends of
//! the runner stream agree on are `af_core`'s `timing`, and the relationships between those
//! and these are asserted below.

use std::time::Duration;

use af_core::timing::{
    MAX_RUN_SECONDS, RENEWAL_WINDOW, STREAM_ANSWER_TIMEOUT, STREAM_IDLE_TIMEOUT,
    STREAM_RECOVERY_BUDGET,
};

mod run;
pub use run::{MIN_HEARTBEAT_SECONDS, MIN_RUN_SECONDS, RunClock, RunSettings};

/// How long an issued run or thread assignment stays valid before the thread
/// becomes reassignable, and the increment each renewal adds.
///
/// Deliberately short: it is the backstop
/// against a runner that dies silently, and a live runner extends it through
/// the renew verb rather than being handed a long run up front.
pub const RUN_TTL: Duration = Duration::from_secs(38);

/// Silence after which a runner is DERIVED offline by a fleet read.
///
/// Three run TTLs: an idle host
/// heartbeats every cycle, and a busy one is reported `busy` by the live-run
/// check before this threshold is ever consulted — so a long execution that
/// stops beating is never mistaken for a dead host.
pub const RUNNER_OFFLINE_AFTER: Duration = RUN_TTL.saturating_mul(3);
/// How often a runner's one clock fires a round: a host heartbeat and, for the runs it
/// holds, their renewal batch.
///
/// The default of the daemon's `heartbeat_interval_seconds`. The daemon serves the value
/// it runs with in each connection's welcome, and the runner rounds on that, so a host
/// never holds its own copy.
/// Strictly below [`RUNNER_OFFLINE_AFTER`], which is what guarantees an idle
/// host beats before a fleet read would derive it offline.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// Continuous healthy native heartbeats required to lift automatic quarantine.
pub const UNCORDON_AFTER: Duration = HEARTBEAT_INTERVAL.saturating_mul(3);

/// The default of `[runner.sandboxes] max_idle_hold_seconds`.
///
/// The longest a daemon lets a runner's frozen sandbox keep protecting its fleet after a
/// run ends. A runner holds for its own span and reports it; a hold past this ceiling
/// protects nothing.
pub const SANDBOX_HOLD_IDLE: Duration = Duration::from_mins(10);

/// How many renewal frames pass between funds checks of one payer.
///
/// A two-round chat then pays two thirds of a check instead of four statements,
/// and a run overruns its payer's balance by at most this many slices.
pub const FUNDS_CHECK_TICKS: u64 = 3;

// The relationships, proven at compile time, as `af_core`'s are.
const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos() + STREAM_ANSWER_TIMEOUT.as_nanos() < RUN_TTL.as_nanos(),
    "a held run's round must find a silent stream before an unrenewed run lapses"
);
const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos() + STREAM_ANSWER_TIMEOUT.as_nanos()
        < STREAM_IDLE_TIMEOUT.as_nanos(),
    "an idle session's beat and its answer must land before the transport closes it"
);
const _: () = assert!(
    HEARTBEAT_INTERVAL.saturating_mul(2).as_nanos() < RUNNER_OFFLINE_AFTER.as_nanos(),
    "a session silent for two heartbeats must close before a fleet read derives its host offline"
);
const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos()
        + STREAM_ANSWER_TIMEOUT.as_nanos()
        + STREAM_RECOVERY_BUDGET.as_nanos()
        + RENEWAL_WINDOW.as_nanos()
        < RUN_TTL.as_nanos(),
    "a failed renewal and bounded reconnect must leave a complete renewal window"
);
const _: () = assert!(
    RENEWAL_WINDOW.as_nanos() < RUN_TTL.as_nanos(),
    "a renewal must be attempted with slack left to retry before the deadline"
);
// A cadence of zero would check once per session and never again, and one as
// long as a run lives would let a run pass its balance unchecked.
const _: () = assert!(
    FUNDS_CHECK_TICKS >= 1 && FUNDS_CHECK_TICKS * HEARTBEAT_INTERVAL.as_secs() < RUN_TTL.as_secs(),
    "funds are checked at least once, and well inside a run's life"
);
const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos() < RUNNER_OFFLINE_AFTER.as_nanos(),
    "an idle host must heartbeat before a fleet read would derive it offline"
);
const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos() < UNCORDON_AFTER.as_nanos()
        && UNCORDON_AFTER.as_nanos() < RUNNER_OFFLINE_AFTER.as_nanos(),
    "lifting quarantine requires several healthy beats before another offline interval"
);
const _: () = assert!(
    RUN_TTL.as_secs() <= MAX_RUN_SECONDS,
    "the default run lifetime must be one a deployment may configure"
);

#[cfg(test)]
mod tests;
