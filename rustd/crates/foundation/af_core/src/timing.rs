//! The lease clock: how long a claim lives, and how long silence is tolerated.
//!
//! Declared here and nowhere else. The daemon sets `leased_until`, treats the
//! same instant as the kill deadline it sends the host, and derives liveness
//! from the lapse threshold. Each runner retains its confirmed lease expiry
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

/// Authoring provider work keeps its existing five-minute budget plus delivery slack.
/// Control ping and renewal deadlines remain independent while the model is pending.
pub const AUTHORING_MODEL_ANSWER_TIMEOUT: Duration = Duration::from_secs(310);

/// How long an issued lease or affinity claim stays valid before the slot
/// becomes reclaimable, and the increment each renewal adds.
///
/// Deliberately short: it is the backstop
/// against a runner that dies silently, and a live runner extends it through
/// the renew verb rather than being handed a long lease up front.
pub const LEASE_TTL: Duration = Duration::from_secs(35);

/// The renewal slack preserved by the lease and connection-recovery relationship.
///
/// Strictly below [`LEASE_TTL`] so a renewal that fails transiently still
/// has room to retry before the deadline.
pub const RENEWAL_WINDOW: Duration = Duration::from_secs(15);

/// How often a runner's supervision loop wakes to consider a renewal.
///
/// Strictly below [`RENEWAL_WINDOW`] so at least one tick lands inside the
/// window.
pub const RENEWAL_TICK: Duration = Duration::from_secs(5);

/// Hard ceiling on one lease's total wall-clock, measured from the lease row's
/// `created_at`.
///
/// Renewal clamps to
/// `min(now + LEASE_TTL, created_at + MAX_RUNTIME)`, so a wedged agent
/// that keeps emitting progress still terminates.
pub const MAX_RUNTIME: Duration = Duration::from_hours(12);

/// How long a stopped engine may close calls and return measured work.
pub const ENGINE_STOP_GRACE: Duration = Duration::from_secs(5);

/// Silence after which a runner is DERIVED offline by a fleet read.
///
/// Three lease TTLs: an idle host
/// heartbeats every cycle, and a busy one is reported `busy` by the live-lease
/// check before this threshold is ever consulted — so a long execution that
/// stops beating is never mistaken for a dead host.
pub const RUNNER_OFFLINE_AFTER: Duration = LEASE_TTL.saturating_mul(3);

/// How often a runner's control loop emits a host heartbeat.
///
/// Served to the runner on every beat, so a host never holds its own copy.
/// Strictly below [`RUNNER_OFFLINE_AFTER`], which is what guarantees an idle
/// host beats before a fleet read would derive it offline.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// How long a runner holds a fleet's sandbox, frozen, after a lease that used
/// it ends processed, for that fleet's next lease to continue in.
///
/// A chat follow-up usually lands within minutes. The runner holds at most as
/// many as it has workers, and a frozen sandbox runs nothing, which is what
/// bounds the cost of the wait.
pub const SANDBOX_HOLD_IDLE: Duration = Duration::from_mins(10);

/// How long an ephemeral wake or runner command may wait for its broker.
///
/// Admission is already durable; a lost wake leaves work for the replica's
/// recovery probe, and heartbeat or renewal checks repair a lost runner command.
pub const READY_WAKE_DEADLINE: Duration = Duration::from_millis(100);

/// Maximum time to answer an application request or finish a partial stream frame.
pub const STREAM_ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
/// Idle interval between application pings; a silent idle path is detected within twenty seconds.
pub const STREAM_PING_INTERVAL: Duration = Duration::from_secs(15);
/// Preferred transport gets this much of the bounded reconnect attempt.
pub const STREAM_HTTP3_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// The TCP fallback gets its own bounded connection attempt.
pub const STREAM_HTTP2_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Ownership confirmation and lane attachment share the remaining recovery budget.
pub const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(3);
/// Complete carrier setup, ownership confirmation and database rebuild budget.
pub const STREAM_RECOVERY_BUDGET: Duration = Duration::from_secs(7);
/// Maximum initial reconnect jitter included in the lease's recovery relationship.
pub const RECONNECT_JITTER_MAX: Duration = Duration::from_secs(2);
/// A connection must remain usable this long before reconnect backoff resets.
pub const STREAM_STABLE_RESET_AFTER: Duration = Duration::from_secs(60);
/// One independent ready-work probe per daemon replica, regardless of broker traffic.
pub const OFFER_FALLBACK_TICK: Duration = Duration::from_secs(5);

const _: () = assert!(
    RENEWAL_TICK.as_nanos()
        + STREAM_ANSWER_TIMEOUT.as_nanos()
        + RECONNECT_JITTER_MAX.as_nanos()
        + STREAM_RECOVERY_BUDGET.as_nanos()
        + RENEWAL_WINDOW.as_nanos()
        < LEASE_TTL.as_nanos(),
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
/// Not a lease value. It is the one spelling of a day for whatever counts in
/// days — a rolling spend window, an invite's life, a `since=` duration — so no
/// caller re-derives it from hours and minutes.
pub const DAY: Duration = Duration::from_hours(24);

// The relationships, proven at compile time. `const` items are evaluated
// whether or not anything reads them, so a value edited out of order fails the
// BUILD rather than being caught by a test somebody might not run.
const _: () = assert!(
    RENEWAL_WINDOW.as_nanos() < LEASE_TTL.as_nanos(),
    "a renewal must be attempted with slack left to retry before the deadline"
);
const _: () = assert!(
    RENEWAL_TICK.as_nanos() < RENEWAL_WINDOW.as_nanos(),
    "at least one supervision tick must land inside the renewal window"
);
const _: () = assert!(
    HEARTBEAT_INTERVAL.as_nanos() < RUNNER_OFFLINE_AFTER.as_nanos(),
    "an idle host must heartbeat before a fleet read would derive it offline"
);
const _: () = assert!(
    LEASE_TTL.as_nanos() < MAX_RUNTIME.as_nanos(),
    "a lease must be renewable at least once before it hits the runtime ceiling"
);
