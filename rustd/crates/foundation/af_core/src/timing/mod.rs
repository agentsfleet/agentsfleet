//! Timing both ends of the runner stream agree on.
//!
//! The answer bound, the reconnect budget and its parts, the transport's idle timeout, the
//! renewal window a run's lifetime must leave, and the longest grant a daemon may configure.
//!
//! The runner and the daemon never depend on each other, so a span both must agree on lives
//! here. A daemon's configurable run clock is `afd_runner`'s `timing`, and each side's own
//! spans live in the crate that uses them.
//!
//! Each is a [`Duration`]: the one span type that a `tokio` sleep takes and
//! that moves a `jiff::Timestamp` (see [`crate::clock::after`]). A span becomes
//! a millisecond count only at a wire field that spells it so, through
//! [`crate::clock::saturating_millis`].
//!
//! The relationships between them are load-bearing, and each is asserted below
//! in a `const` block rather than left to a comment.

use std::time::Duration;

/// The renewal slack preserved by the run and connection-recovery relationship.
///
/// Strictly below a daemon's run lifetime (`afd_runner`'s `timing`) so a renewal that
/// fails transiently still has room to retry before the deadline.
/// Kept: without it a live runner's run lapses and is handed to another runner.
pub const RENEWAL_WINDOW: Duration = Duration::from_secs(15);

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

/// Longest run lifetime a deployment may configure: one hour per grant.
///
/// A bound on one renewal slice, not on a run. A run renews for as long as its
/// fleet's budget covers it and nobody stops it.
pub const MAX_RUN_SECONDS: u64 = Duration::from_hours(1).as_secs();

/// One day.
///
/// Not a run value. It is the one spelling of a day for whatever counts in
/// days — a rolling spend window, an invite's life, a `since=` duration — so no
/// caller re-derives it from hours and minutes.
pub const DAY: Duration = Duration::from_hours(24);

// The relationships, proven at compile time. `const` items are evaluated whether or not
// anything reads them, so a value edited out of order fails the BUILD rather than being
// caught by a test somebody might not run.
const _: () = assert!(
    STREAM_HTTP3_CONNECT_TIMEOUT.as_nanos()
        + STREAM_HTTP2_CONNECT_TIMEOUT.as_nanos()
        + STREAM_OPEN_TIMEOUT.as_nanos()
        == STREAM_RECOVERY_BUDGET.as_nanos(),
    "the complete reconnect budget includes both carriers and ownership rebuild"
);
const _: () = assert!(
    STREAM_ANSWER_TIMEOUT.as_nanos() < RENEWAL_WINDOW.as_nanos(),
    "a retry round, one answer bound after a refused one, must land inside the renewal window"
);
