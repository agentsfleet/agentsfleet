//! N workers, each polling for a lease and running it.
//!
//! The pool grows to the assigned worker count and never kills a worker: one
//! numbered past a smaller count finishes its lease and then waits, so a
//! shrinking assignment never interrupts a run. Shutdown stops polling; leases
//! in flight run to their reports.

use std::sync::Arc;
use std::time::Duration;

use afd_core::timing::NO_WORK_RETRY_AFTER_MS;
use afd_wire::lease::LeaseResponse;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::heartbeat::Assignment;
use crate::lease_loop::Lessee;
use crate::turns::FleetTurns;

const EVENT_STARTED: &str = "worker_started";
const EVENT_STOPPED: &str = "worker_stopped";
const EVENT_POLL_FAILED: &str = "lease_read_failed";
const EVENT_UNAUTHORIZED: &str = "lease_unauthorized";
const EVENT_LEASE_ERROR: &str = "lease_write_failed";

/// Runs workers to the assigned count until `shutdown`, then waits for every
/// lease in flight.
pub async fn serve(
    lessee: Arc<Lessee>,
    mut assignment: watch::Receiver<Assignment>,
    shutdown: CancellationToken,
) {
    let (turns, coordinator) = FleetTurns::start();
    let coordinator = tokio::spawn(coordinator);
    let mut workers = JoinSet::new();
    let mut spawned = 0;
    loop {
        let wanted = assignment.borrow_and_update().workers.get();
        while spawned < wanted {
            let worker = Worker {
                number: spawned,
                lessee: Arc::clone(&lessee),
                turns: turns.clone(),
                assignment: assignment.clone(),
                shutdown: shutdown.clone(),
            };
            workers.spawn(worker.run());
            spawned += 1;
        }
        tokio::select! {
            () = shutdown.cancelled() => break,
            changed = assignment.changed() => if changed.is_err() { break },
        }
    }
    workers.join_all().await;
    drop(turns);
    drop(coordinator.await);
}

/// One worker: wait to be wanted, poll, run, repeat.
struct Worker {
    number: u32,
    lessee: Arc<Lessee>,
    turns: FleetTurns,
    assignment: watch::Receiver<Assignment>,
    shutdown: CancellationToken,
}

impl Worker {
    async fn run(mut self) {
        let worker = self.number;
        let event = EVENT_STARTED;
        tracing::info!(worker, event);
        while let Some(pause) = self.next().await {
            tokio::select! {
                () = self.shutdown.cancelled() => break,
                () = tokio::time::sleep(pause) => {}
            }
        }
        let event = EVENT_STOPPED;
        tracing::info!(worker, event);
    }

    /// Waits to be wanted, then polls once and runs what it got. Returns how
    /// long to pause before the next poll, or `None` to stop.
    async fn next(&mut self) -> Option<Duration> {
        let number = self.number;
        tokio::select! {
            () = self.shutdown.cancelled() => return None,
            wanted = self.assignment.wait_for(|now| now.takes_work(number)) => wanted.ok()?,
        };
        let polled = tokio::select! {
            () = self.shutdown.cancelled() => return None,
            polled = self.lessee.plane.lease() => polled,
        };
        let body = match polled {
            Ok(body) => body,
            Err(failure) if failure.is_unauthorized() => {
                let code = failure.code().as_str();
                let event = EVENT_UNAUTHORIZED;
                tracing::error!(
                    error_code = code,
                    event,
                    "the daemon refused this runner's token"
                );
                self.shutdown.cancel();
                return None;
            }
            Err(failure) => return Some(idle(&failure, EVENT_POLL_FAILED)),
        };
        let reply = match body.decode::<LeaseResponse<'_>>() {
            Ok(reply) => reply,
            Err(failure) => return Some(idle(&failure, EVENT_POLL_FAILED)),
        };
        let Some(lease) = reply.lease else {
            let hinted = reply.retry_after_ms.unwrap_or(NO_WORK_RETRY_AFTER_MS);
            return Some(Duration::from_millis(u64::from(hinted)));
        };
        if let Err(failure) = self.lessee.run(&self.turns, &lease).await {
            return Some(idle(&failure, EVENT_LEASE_ERROR));
        }
        Some(Duration::ZERO)
    }
}

/// Logs a failed poll or lease, and pauses before the next.
fn idle(failure: &crate::Error, event: &'static str) -> Duration {
    let code = failure.code().as_str();
    tracing::warn!(
        error_code = code,
        event,
        "the worker pauses before polling again"
    );
    Duration::from_millis(u64::from(NO_WORK_RETRY_AFTER_MS))
}

#[cfg(test)]
#[path = "worker_pool/tests.rs"]
mod tests;
