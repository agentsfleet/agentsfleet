//! Held reports, posted again until the daemon answers each.
//!
//! A lease posts its own report once. When that answer could still change — a
//! deploy in progress, a 5xx, a rate limit — the report stays spooled and the
//! drain takes it over: it posts every held report, backs off with jitter
//! while any is still held, and otherwise sleeps until a lease hands it one.
//! At boot its first pass is the replay of whatever a dead process left.

use std::time::Duration;

use afd_core::id::Uuid7;
use tokio::sync::Notify;

use crate::client::{ControlPlane, endless};
use crate::halt::Halt;
use crate::holds::Holds;
use crate::report_spool::{Delivery, ReportSpool, Spooled};

const EVENT_REPLAY_FAILED: &str = "report_spool_replay_failed";

/// The drain, over one spool.
#[derive(Debug)]
pub(crate) struct Drainer<'a> {
    /// Where held reports are.
    pub(crate) spool: &'a ReportSpool,
    /// Where they go.
    pub(crate) plane: &'a ControlPlane,
    /// When to stop.
    pub(crate) halt: &'a Halt,
    /// Rung when a lease leaves a report held.
    pub(crate) held: &'a Notify,
    /// The sandboxes leases left held, one of which a superseded report's
    /// lease may have parked.
    pub(crate) holds: &'a Holds,
}

impl Drainer<'_> {
    /// Drains until the runner stops serving.
    pub(crate) async fn run(self) {
        let mut pauses = endless();
        loop {
            let pause = if self.pass().await {
                pauses.next()
            } else {
                pauses = endless();
                None
            };
            tokio::select! {
                () = self.halt.serving().cancelled() => return,
                () = sleep_or_wait(pause) => {}
                () = self.held.notified() => {}
            }
        }
    }

    /// Posts every held report once. Returns whether any is still held.
    async fn pass(&self) -> bool {
        let pending = match self.spool.pending().await {
            Ok(pending) => pending,
            Err(failure) => {
                replay_failed(&failure);
                return true;
            }
        };
        let mut still_held = false;
        for spooled in pending {
            match spooled.deliver(self.plane).await {
                Ok(Delivery::Settled) => {}
                Ok(Delivery::Superseded | Delivery::Rejected) => self.superseded(&spooled),
                Ok(Delivery::Kept(failure)) => {
                    if self.halt.stops_on(&failure) {
                        return true;
                    }
                    still_held = true;
                }
                Err(failure) => {
                    replay_failed(&failure);
                    still_held = true;
                }
            }
        }
        still_held
    }

    /// Ends the hold `spooled`'s lease parked, as the lease's own post does
    /// when the daemon settled the lease without the report or cannot read
    /// it: that sandbox carries a run the daemon never recorded, and serves no
    /// next lease. A file no lease id names parked nothing.
    fn superseded(&self, spooled: &Spooled) {
        if let Ok(lease) = Uuid7::parse(spooled.lease_id()) {
            self.holds.supersede(lease);
        }
    }
}

/// Sleeps for `pause`, or until woken when there is nothing to retry.
async fn sleep_or_wait(pause: Option<Duration>) {
    match pause {
        Some(pause) => tokio::time::sleep(pause).await,
        None => std::future::pending().await,
    }
}

/// Logs a spool the drain could not read or settle.
fn replay_failed(failure: &crate::Error) {
    let code = failure.code().as_str();
    let event = EVENT_REPLAY_FAILED;
    tracing::warn!(
        error_code = code,
        event,
        "a held report waits for the next pass"
    );
}

#[cfg(test)]
#[path = "drainer/tests.rs"]
mod tests;
