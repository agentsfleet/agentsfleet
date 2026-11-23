//! The run clock: how long an assignment lives, and how long silence is tolerated.
//!
//! Defaults are declared here. The validated deployment clock sets `held_until` and treats the
//! same instant as the kill deadline it sends the host, and derives liveness
//! from the lapse threshold. Each runner retains its confirmed run expiry
//! and uses the heartbeat cadence advertised by the daemon.
//!
//! Each is a [`Duration`]: the one span type that a `tokio` sleep takes and
//! that moves a `jiff::Timestamp` (see [`crate::clock::after`]). A span becomes
//! a millisecond count only at a wire field that spells it so, through
//! [`crate::clock::saturating_millis`].
//!
//! The relationships between them are load-bearing, and each is asserted below
//! in a `const` block rather than left to a comment.

use std::time::Duration;

mod run;
pub use run::{
    MAX_RUN_SECONDS, MIN_HEARTBEAT_SECONDS, MIN_RUN_SECONDS, RENEWAL_MARGIN_SECONDS, RunClock,
    RunSettings,
};

/// How long an issued run or thread assignment stays valid before the thread
/// becomes reassignable, and the increment each renewal adds.
///
/// Deliberately short: it is the backstop
/// against a runner that dies silently, and a live runner extends it through
/// the renew verb rather than being handed a long run up front.
pub const RUN_TTL: Duration = Duration::from_secs(38);

/// The renewal slack preserved by the run and connection-recovery relationship.
///
/// Strictly below [`RUN_TTL`] so a renewal that fails transiently still
/// has room to retry before the deadline.
/// Kept: without it a live runner's run lapses and is handed to another runner.
pub const RENEWAL_WINDOW: Duration = Duration::from_secs(15);

/// How long a stopped engine may close calls and return measured work.
pub const ENGINE_STOP_GRACE: Duration = Duration::from_secs(5);

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
/// Served to the runner on every beat, so a host never holds its own copy.
/// Strictly below [`RUNNER_OFFLINE_AFTER`], which is what guarantees an idle
/// host beats before a fleet read would derive it offline.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// Continuous healthy native heartbeats required to lift automatic quarantine.
pub const UNCORDON_AFTER: Duration = HEARTBEAT_INTERVAL.saturating_mul(3);

/// How long a runner holds a fleet's sandbox, frozen, after a run that used
/// it ends processed, for that fleet's next run to continue in.
///
/// A chat follow-up usually lands within minutes. The runner holds at most as
/// many as it has workers, and a frozen sandbox runs nothing, which is what
/// bounds the cost of the wait.
pub const SANDBOX_HOLD_IDLE: Duration = Duration::from_mins(10);

/// How long an ephemeral NATS wake or runner command may wait for its broker.
///
/// The turn is already durable. A wake or runner command the broker loses is owed as
/// a gap, whose recovery pass and re-check find the work, and every booting daemon
/// gaps every replica once.
/// Kept: without it every committed write's request hangs while NATS is down.
pub const READY_WAKE_DEADLINE: Duration = Duration::from_millis(100);

/// Maximum time to answer an application request or finish a partial stream frame.
pub const STREAM_ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the runner stream's transport keeps an idle connection open: both carriers
/// close a session that carried nothing for this long, with no keep-alive.
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Preferred transport gets this much of the bounded reconnect attempt.
/// Kept: a network that drops UDP would otherwise never reach the TCP fallback.
pub const STREAM_HTTP3_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// The TCP fallback gets its own bounded connection attempt.
///
/// Kept: an attempt outliving the daemon's 5-second TLS accept reads its
/// hang-up as a refused handshake, which ends the runner instead of retrying.
pub const STREAM_HTTP2_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Ownership confirmation and lane attachment share the remaining recovery budget.
/// Kept: the daemon holds a connection permit for an opening, and a peer that
/// never says hello would hold it forever.
pub const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(3);
/// Complete carrier setup, ownership confirmation and database rebuild budget.
/// Kept: a stuck attempt is abandoned so a fresh one fits inside a held run's lease.
pub const STREAM_RECOVERY_BUDGET: Duration = Duration::from_secs(7);
/// A connection must remain usable this long before reconnect backoff resets.
pub const STREAM_STABLE_RESET_AFTER: Duration = Duration::from_secs(60);
/// The shortest idle timeout a proxy or load balancer on the runner's path is assumed to apply.
///
/// Nothing reads it at run time. It is the floor [`HEARTBEAT_INTERVAL`] is held under, so an
/// idle stream carries a beat before a common 60-second middlebox timeout closes it.
pub const PROXY_IDLE_FLOOR: Duration = Duration::from_secs(60);

const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos() + STREAM_ANSWER_TIMEOUT.as_nanos() < RUN_TTL.as_nanos(),
    "a held run's round must find a silent stream before an unrenewed run lapses"
);

const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos() < PROXY_IDLE_FLOOR.as_nanos(),
    "an idle stream must carry a beat before a proxy's idle timeout closes it"
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
    STREAM_HTTP3_CONNECT_TIMEOUT.as_nanos()
        + STREAM_HTTP2_CONNECT_TIMEOUT.as_nanos()
        + STREAM_OPEN_TIMEOUT.as_nanos()
        == STREAM_RECOVERY_BUDGET.as_nanos(),
    "the complete reconnect budget includes both carriers and ownership rebuild"
);

/// One day.
///
/// Not a run value. It is the one spelling of a day for whatever counts in
/// days — a rolling spend window, an invite's life, a `since=` duration — so no
/// caller re-derives it from hours and minutes.
pub const DAY: Duration = Duration::from_hours(24);

// The relationships, proven at compile time. `const` items are evaluated
// whether or not anything reads them, so a value edited out of order fails the
// BUILD rather than being caught by a test somebody might not run.
const _: () = assert!(
    RENEWAL_WINDOW.as_nanos() < RUN_TTL.as_nanos(),
    "a renewal must be attempted with slack left to retry before the deadline"
);
const _: () = assert!(
    STREAM_ANSWER_TIMEOUT.as_nanos() < RENEWAL_WINDOW.as_nanos(),
    "a retry round, one answer bound after a refused one, must land inside the renewal window"
);

/// How many renewal frames pass between funds checks of one payer.
///
/// A two-round chat then pays two thirds of a check instead of four statements,
/// and a run overruns its payer's balance by at most this many slices.
pub const FUNDS_CHECK_TICKS: u64 = 3;

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
