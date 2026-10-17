//! The lease clock: how long a claim lives, and how long silence is tolerated.
//!
//! Declared here and nowhere else. The daemon sets `leased_until`, treats the
//! same instant as the kill deadline it sends the host, and derives liveness
//! from the lapse threshold — so every one of these is a decision this side
//! makes. The runner is told the two it needs, its deadline on the renew reply
//! and its cadence on the beat, and holds neither.
//!
//! Each is a [`Duration`]: the one span type that a `tokio` sleep takes and
//! that moves a `jiff::Timestamp` (see [`crate::clock::after`]). A span becomes
//! a millisecond count only at a wire field that spells it so, through
//! [`crate::clock::saturating_millis`].
//!
//! The relationships between them are load-bearing, and each is asserted below
//! in a `const` block rather than left to a comment.

use std::time::Duration;

/// How long an issued lease or affinity claim stays valid before the slot
/// becomes reclaimable, and the increment each renewal adds.
///
/// Deliberately short: it is the backstop
/// against a runner that dies silently, and a live runner extends it through
/// the renew verb rather than being handed a long lease up front.
pub const LEASE_TTL: Duration = Duration::from_secs(30);

/// How long before expiry a runner auto-renews.
///
/// Strictly below [`LEASE_TTL`] so a renewal that fails transiently still
/// has room to retry before the deadline.
pub const RENEWAL_WINDOW: Duration = Duration::from_secs(10);

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

/// The backoff hint handed to a runner that found no work.
///
/// The lease verb always answers 200 — never 204 — and this rides the reply as
/// `retry_after_ms`.
pub const NO_WORK_RETRY_AFTER: Duration = Duration::from_secs(1);

/// How long a readiness-doorbell command may take before its caller goes on
/// without it: the ring a mark publishes, and the peek of the partition a ring
/// named.
///
/// The doorbell only ever makes a poll faster. Under the datastore's own
/// request timeout a slow node could hold an admission, or a poll the rotation
/// alone would have answered, for seconds; past this deadline the ring is
/// reported lost or the rung peek counts as failed, and the rotation still
/// reaches the work.
pub const READY_WAKE_DEADLINE: Duration = Duration::from_millis(100);

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
