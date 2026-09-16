//! Letting the requests already in flight finish, before the process exits.
//!
//! # What a deployment used to cost
//!
//! Every connection task selected on the SAME token the supervisor cancels, so
//! a signal did not stop the server — it cut it. A request halfway through a
//! steer had its future dropped mid-await: the caller saw a closed connection
//! with no status, and had no way to tell a request that never ran from one
//! that ran and whose answer it never got. On a rolling deploy that is not a
//! rare race, it is every replaced replica, every time.
//!
//! # Two tokens, because the two questions are different
//!
//! "Stop taking new work" and "abandon the work you have" are separate events
//! with a bounded interval between them, and collapsing them into one token is
//! what made a stop into a cut. So:
//!
//! 1. `accepting` is cancelled. The accept loop breaks, drops its listener, and
//!    the port stops answering — a new connection is refused, immediately and
//!    by the kernel, which is the only refusal that needs no code.
//! 2. The in-flight count is awaited, bounded by [`DRAIN_TIMEOUT`]. Each live
//!    connection holds a [`Guard`]; the count falls as they finish.
//! 3. Only then does the supervisor cancel, which is what abandons anything
//!    still running. A request that outlasts the bound is still cut — but it is
//!    cut after a stated interval and named in a log line, rather than silently
//!    at the instant the signal landed.
//!
//! The bound is not optional. Without it one request that never completes holds
//! a deployment open forever, and an operator watching a pod that will not
//! terminate has strictly less information than one reading
//! `drain_timed_out in_flight=1`.
//!
//! # Why a counter and not a `TaskTracker`
//!
//! `tokio_util::task::TaskTracker` does exactly this, behind the crate's `rt`
//! feature, which this workspace does not enable. Turning it on for one call
//! site would widen a workspace-wide dependency to narrow a file; the counter
//! below is thirty lines and reports the number a drain rehearsal has to record
//! anyway.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// How long in-flight requests may take to finish before the drain gives up.
///
/// A deadline at the call site, per Invariant 4. Shorter than the supervisor's
/// [`crate::supervisor::JOIN_TIMEOUT`] would make the drain the thing that fails
/// first on a slow host; the two are independent bounds on different phases, and
/// this one covers a request rather than a background task.
pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(15);

/// What one drain did, for the log line and for a rehearsal's notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settled {
    /// Connections still being served when accepting stopped.
    pub in_flight_at_close: usize,
    /// Connections still being served when the drain gave up. Zero is a clean
    /// drain; anything else is what the supervisor then cut.
    pub abandoned: usize,
}

impl Settled {
    /// Nothing was in flight and nothing was cut — what a daemon with no accept
    /// loop drains, and the identity a background-only shape reports.
    pub const EMPTY: Self = Self {
        in_flight_at_close: 0,
        abandoned: 0,
    };

    /// True when every in-flight request finished inside the bound.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.abandoned == 0
    }
}

/// The accept side's stop signal and the in-flight count, together.
///
/// Cloning shares both — every clone names the same drain, which is the point:
/// the accept loop holds one, each connection takes a [`Guard`] from another,
/// and the shutdown path awaits the same count they are moving.
#[derive(Clone, Debug)]
pub struct Drain {
    accepting: CancellationToken,
    /// Cancelled by the accept loop once it has actually left the loop.
    ///
    /// Distinct from [`Self::accepting`] because cancelling that one only ASKS:
    /// the loop is parked in `accept()` inside a `select!` and leaves on its
    /// next poll, so a connection can still be accepted and counted in between.
    /// This says it has happened.
    stopped: CancellationToken,
    /// Whether an accept loop is running at all, so [`Self::settle`] knows
    /// whether anything will ever cancel `stopped`. A `Drain` with no loop —
    /// most of the sibling test module, and a daemon that never bound a
    /// listener — must still settle rather than wait forever.
    attached: Arc<AtomicBool>,
    live: Arc<AtomicUsize>,
    idle: Arc<Notify>,
}

impl Drain {
    /// A drain that is accepting and has nothing in flight.
    #[must_use]
    pub fn new() -> Self {
        Self {
            accepting: CancellationToken::new(),
            stopped: CancellationToken::new(),
            attached: Arc::new(AtomicBool::new(false)),
            live: Arc::new(AtomicUsize::new(0)),
            idle: Arc::new(Notify::new()),
        }
    }

    /// The token the accept loop selects on. Cancelled by [`Self::settle`], and
    /// deliberately NOT the token that abandons a connection in flight.
    #[must_use]
    pub fn accepting(&self) -> &CancellationToken {
        &self.accepting
    }

    /// Declares that an accept loop is running against this drain.
    ///
    /// Called once, by the loop, before it takes its first connection.
    pub fn attach(&self) {
        self.attached.store(true, Ordering::Release);
    }

    /// Declares that the accept loop has left the loop and dropped its listener.
    ///
    /// Called however the loop ends, which is what lets [`Self::settle`] read a
    /// count no further connection can join.
    pub fn stopped_accepting(&self) {
        self.stopped.cancel();
    }

    /// Claim a slot for one connection. The returned guard must be held for as
    /// long as the connection is being served: its `Drop` is what lets a drain
    /// finish, so dropping it early reports a connection as done while it runs.
    #[must_use]
    pub fn enter(&self) -> Guard {
        self.live.fetch_add(1, Ordering::AcqRel);
        Guard {
            live: Arc::clone(&self.live),
            idle: Arc::clone(&self.idle),
        }
    }

    /// How many connections are being served right now.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.live.load(Ordering::Acquire)
    }

    /// Stop accepting, then wait up to `bound` for the in-flight count to reach
    /// zero. Returns what it found, whether or not it got there.
    ///
    /// Infallible on purpose: a drain that timed out has not failed, it has
    /// finished with something left, and `Settled` says how much. A `Result`
    /// here would make every caller decide what an expired bound means, and the
    /// answer is always the same one — carry on to the supervisor.
    pub async fn settle(&self, bound: Duration) -> Settled {
        self.accepting.cancel();
        // Cancelling only ASKS the loop to stop; it is parked in `accept()` and
        // leaves on its next poll, so a connection can still be accepted and
        // counted in between. Waiting for the loop to say it has left is what
        // makes `in_flight_at_close` the count at CLOSE rather than the count
        // when close was requested. Skipped when no loop ever attached, which
        // would otherwise wait for a cancellation nobody is going to make.
        if self.attached.load(Ordering::Acquire) {
            self.stopped.cancelled().await;
        }
        let in_flight_at_close = self.in_flight();
        if in_flight_at_close > 0 {
            tracing::info!(
                in_flight = in_flight_at_close,
                bound_ms = u64::try_from(bound.as_millis()).unwrap_or(u64::MAX),
                event = EVENT_DRAIN_STARTED,
                "no longer accepting; waiting for in-flight requests"
            );
        }
        // A timeout around the wait, not inside it: the wait itself has no
        // failure mode, so the bound is the only thing that can end it early.
        drop(tokio::time::timeout(bound, self.idle()).await);
        let abandoned = self.in_flight();
        if abandoned > 0 {
            tracing::warn!(
                error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str(),
                in_flight = abandoned,
                event = EVENT_DRAIN_TIMED_OUT,
                "drain bound expired; in-flight requests will be cut"
            );
        }
        Settled {
            in_flight_at_close,
            abandoned,
        }
    }

    /// Resolves once nothing is in flight.
    ///
    /// The re-check after registering is load-bearing: `Notify` wakes only a
    /// waiter that was already registered, so a guard dropped between the load
    /// and the registration would notify nobody and this would wait out the
    /// whole bound with an empty server. `enable()` registers first, then the
    /// count is read again.
    async fn idle(&self) {
        loop {
            if self.in_flight() == 0 {
                return;
            }
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.in_flight() == 0 {
                return;
            }
            notified.await;
        }
    }
}

impl Default for Drain {
    fn default() -> Self {
        Self::new()
    }
}

/// One connection's claim on the drain, released by dropping it.
///
/// `Drop` rather than an explicit `release`, because the connection task can end
/// by returning, by an error, or by being cancelled mid-await, and only `Drop`
/// covers all three. A count that leaked on the cancelled path would make every
/// drain after the first one wait out its whole bound.
#[derive(Debug)]
pub struct Guard {
    live: Arc<AtomicUsize>,
    idle: Arc<Notify>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        // `fetch_sub` returns the PREVIOUS value, so 1 means this guard was the
        // last one. Notifying only then keeps a busy server from waking the
        // waiter on every completed request.
        if self.live.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.idle.notify_waiters();
        }
    }
}

/// The drain's two stable event names. One spelling each: a log pipeline
/// selects on these, and a second spelling of a terminal event is a silently
/// missing alert.
const EVENT_DRAIN_STARTED: &str = "drain_started";
const EVENT_DRAIN_TIMED_OUT: &str = "drain_timed_out";

#[cfg(test)]
mod tests;
