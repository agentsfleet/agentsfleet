//! The background work no request pays for.
//!
//! Four sweepers keep the runner plane honest between requests: a runner that
//! stopped beating has to be noticed, a lease its holder abandoned has to
//! become leasable again, history has to stop growing, and a repair a human
//! approved has to be dispatched. None of them belongs on a request path —
//! every one is unbounded work on someone else's row.
//!
//! # One loop, four sweepers
//!
//! Each Zig sweeper carries its own `run`: a `while (!shutdown.load(.acquire))`
//! around one bounded pass, and a `sleepInterruptible` that wakes every 100ms
//! to re-read an atomic it usually finds unchanged. Four copies of the same
//! twenty lines, and every one of them pays up to a tenth of a second of
//! shutdown latency for the privilege.
//!
//! [`run`] is that loop, once, generic over what it drives. Cancellation is a
//! [`CancellationToken`] selected against the sleep, so a stopping daemon
//! interrupts the WAIT rather than waiting out a poll interval — the property
//! `supervisor.rs` records for the daemon's tasks, applied to the tasks
//! themselves.
//!
//! # A failed pass is never a stopped sweeper
//!
//! Every sweep here is idempotent and bounded, so a pass that fails has cost
//! nothing a later pass cannot redo. A sweeper that exited on a datastore blip
//! would need an operator to restart the daemon to get liveness back, which is
//! a worse outage than the one it reported.

pub mod liveness;
pub mod reclaim;
pub mod repair;
pub mod retention;

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::error::Result;

/// The event a completed pass is logged under.
const EVENT_SWEPT: &str = "sweep_completed";

/// The event a failed pass is logged under.
const EVENT_FAILED: &str = "sweep_failed";

/// The event a starting sweeper is logged under.
const EVENT_STARTED: &str = "sweeper_started";

/// The event a stopping sweeper is logged under.
const EVENT_STOPPED: &str = "sweeper_stopped";

/// One bounded pass over rows nobody is waiting on.
///
/// A trait rather than four loops, and generic rather than `dyn`: [`run`] is
/// instantiated once per sweeper at compile time, so nothing here is boxed and
/// each sweeper's future keeps its own concrete type.
pub trait Sweep: Send + Sync + 'static {
    /// What this sweeper is called, in the log line that reports it.
    fn name(&self) -> &'static str;

    /// How long to wait before the next pass.
    ///
    /// A property of the sweeper rather than a parameter of [`run`], because
    /// the interval is part of what the sweeper IS: liveness paces itself
    /// against the heartbeat interval, and retention has no reason to run more
    /// than hourly.
    ///
    /// Asked again before every wait, so a sweeper may VARY it — retention
    /// shortens its own gap while a backlog drains, and the loop needs to know
    /// nothing about retention for that to work.
    fn interval(&self) -> Duration;

    /// Performs one pass.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. Nothing else: a sweep that
    /// found no work is `Ok` with a zero tally, because "nothing to do" is the
    /// ordinary answer and not a fault to log.
    fn sweep(&self) -> impl Future<Output = Result<Swept>> + Send;
}

/// What one pass did.
///
/// Reported rather than returned to a caller — no request is waiting — so this
/// exists for the log line an operator reads. Rows SCANNED is separate from
/// rows CHANGED on purpose: a sweeper scanning steadily and changing nothing is
/// healthy, and one scanning nothing at all is a sweeper whose query stopped
/// matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Swept {
    /// Rows this pass considered.
    pub scanned: u64,
    /// Rows it changed.
    pub changed: u64,
}

impl Swept {
    /// Whether this pass is worth a line.
    ///
    /// A sweeper waking every ten seconds over an idle deployment would
    /// otherwise write a log line every ten seconds saying it did nothing.
    const fn is_worth_reporting(self) -> bool {
        self.scanned > 0
    }
}

/// Runs `sweeper` until `token` is cancelled.
///
/// The wait is SELECTED against cancellation rather than polled, so a stopping
/// daemon interrupts it immediately. Ordering is deliberate: the wait comes
/// first, so a sweeper does not race the boot it was spawned during — every
/// datastore it touches has been proven to answer by then, but the rows it
/// reads may still be mid-migration.
pub async fn run<S: Sweep>(sweeper: S, token: CancellationToken) {
    let name = sweeper.name();
    tracing::debug!(
        sweeper = name,
        interval_ms = sweeper.interval().as_millis(),
        event = EVENT_STARTED,
        "a sweeper started"
    );

    loop {
        // Read EVERY tick, not once at start-up: retention shortens its own
        // interval while a backlog drains, and the loop learns that by asking
        // rather than by knowing anything about retention.
        tokio::select! {
            () = token.cancelled() => break,
            () = tokio::time::sleep(sweeper.interval()) => {}
        }
        match sweeper.sweep().await {
            Ok(swept) if swept.is_worth_reporting() => tracing::debug!(
                sweeper = name,
                scanned = swept.scanned,
                changed = swept.changed,
                event = EVENT_SWEPT,
                "a sweep completed"
            ),
            Ok(_idle) => {}
            // Reported and continued. Every sweep here is idempotent and
            // bounded, so a failed pass has cost nothing a later pass cannot
            // redo — and a sweeper that exited on a datastore blip would need a
            // daemon restart to get liveness back.
            Err(failure) => tracing::warn!(
                sweeper = name,
                error_code = failure.code().as_str(),
                error = %failure,
                event = EVENT_FAILED,
                "a sweep failed; the next pass will retry it"
            ),
        }
    }

    tracing::debug!(sweeper = name, event = EVENT_STOPPED, "a sweeper stopped");
}

#[cfg(test)]
mod tests;
