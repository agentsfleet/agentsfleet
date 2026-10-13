//! The lease clock: how long a claim lives, and how long silence is tolerated.
//!
//! Declared here and nowhere else. The daemon sets `leased_until`, treats the
//! same instant as the kill deadline it sends the host, and derives liveness
//! from the lapse threshold — so every one of these is a decision this side
//! makes. The runner is told the two it needs, its deadline on the renew reply
//! and its cadence on the beat, and holds neither.
//!
//! # Why milliseconds and not [`std::time::Duration`]
//!
//! Every one of these is compared against a `bigint` column holding epoch
//! milliseconds, or arrives on the wire as one. A `Duration` would be converted
//! at each of those boundaries, and a conversion is where a unit is lost. The
//! type that carries an INSTANT is [`crate::clock::UnixMillis`]; these are the
//! spans it is moved by, and they stay in the units the rows are in.
//!
//! The relationships between them are load-bearing, and each is asserted below
//! in a `const` block rather than left to a comment.

/// How long an issued lease or affinity claim stays valid before the slot
/// becomes reclaimable, and the increment each renewal adds.
///
/// Deliberately short: it is the backstop
/// against a runner that dies silently, and a live runner extends it through
/// the renew verb rather than being handed a long lease up front.
pub const LEASE_TTL_MS: i64 = 30_000;

/// How long before expiry a runner auto-renews.
///
/// Strictly below [`LEASE_TTL_MS`] so a renewal that fails transiently still
/// has room to retry before the deadline.
pub const RENEWAL_WINDOW_MS: i64 = 10_000;

/// How often a runner's supervision loop wakes to consider a renewal.
///
/// Strictly below [`RENEWAL_WINDOW_MS`] so at least one tick lands inside the
/// window.
pub const RENEWAL_TICK_MS: i64 = 5_000;

/// Hard ceiling on one lease's total wall-clock, measured from the lease row's
/// `created_at`.
///
/// Renewal clamps to
/// `min(now + LEASE_TTL_MS, created_at + MAX_RUNTIME_MS)`, so a wedged agent
/// that keeps emitting progress still terminates.
pub const MAX_RUNTIME_MS: i64 = 43_200_000;

/// Silence after which a runner is DERIVED offline by a fleet read.
///
/// Three lease TTLs: an idle host
/// heartbeats every cycle, and a busy one is reported `busy` by the live-lease
/// check before this threshold is ever consulted — so a long execution that
/// stops beating is never mistaken for a dead host.
pub const RUNNER_OFFLINE_AFTER_MS: i64 = LEASE_TTL_MS * 3;

/// How often a runner's control loop emits a host heartbeat.
///
/// Served to the runner on every beat, so a host never holds its own copy.
/// Strictly below [`RUNNER_OFFLINE_AFTER_MS`], which is what guarantees an idle
/// host beats before a fleet read would derive it offline.
pub const HEARTBEAT_INTERVAL_MS: i64 = 10_000;

/// How long a runner holds a fleet's sandbox, frozen, after a lease that used
/// it ends processed, for that fleet's next lease to continue in.
///
/// A chat follow-up usually lands within minutes. The runner holds at most as
/// many as it has workers, and a frozen sandbox runs nothing, which is what
/// bounds the cost of the wait.
pub const SANDBOX_HOLD_IDLE_MS: i64 = 600_000;

/// The backoff hint handed to a runner that found no work.
///
/// The lease verb always answers 200 — never 204 — and this rides the reply as
/// `retry_after_ms`.
pub const NO_WORK_RETRY_AFTER_MS: u32 = 1_000;

/// One day, in the milliseconds every span here is counted in.
///
/// Not a lease value. It is the one spelling of a day for whatever counts in
/// days — a rolling spend window, an invite's life, a `since=` duration — so no
/// caller re-derives it from hours and minutes.
pub const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

// The relationships, proven at compile time. `const` items are evaluated
// whether or not anything reads them, so a value edited out of order fails the
// BUILD rather than being caught by a test somebody might not run.
const _: () = assert!(
    RENEWAL_WINDOW_MS < LEASE_TTL_MS,
    "a renewal must be attempted with slack left to retry before the deadline"
);
const _: () = assert!(
    RENEWAL_TICK_MS < RENEWAL_WINDOW_MS,
    "at least one supervision tick must land inside the renewal window"
);
const _: () = assert!(
    HEARTBEAT_INTERVAL_MS < RUNNER_OFFLINE_AFTER_MS,
    "an idle host must heartbeat before a fleet read would derive it offline"
);
const _: () = assert!(
    LEASE_TTL_MS < MAX_RUNTIME_MS,
    "a lease must be renewable at least once before it hits the runtime ceiling"
);
