//! N workers, each polling for a lease and running it.
//!
//! The pool grows to the assigned worker count and never kills a worker: one
//! numbered past a smaller count finishes its lease and then waits, so a
//! shrinking assignment never interrupts a run. A worker that panics is logged
//! and started again under its number. When leasing stops, polling stops;
//! leases in flight run to their reports.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use afd_core::error_code;
use afd_core::timing::NO_WORK_RETRY_AFTER_MS;
use afd_wire::lease::LeaseResponse;
use backon::ExponentialBackoff;
use tokio::sync::watch;
use tokio::task::{Id, JoinError, JoinSet};

use crate::client::endless;
use crate::heartbeat::Assignment;
use crate::lease_loop::Lessee;
use crate::turns::FleetTurns;

/// The shortest pause after an empty poll, whatever the daemon hints: a hint
/// of zero must not turn polling into a busy loop.
pub(crate) const MIN_POLL_PAUSE: Duration = Duration::from_millis(250);
const EVENT_STARTED: &str = "worker_started";
const EVENT_STOPPED: &str = "worker_stopped";
const EVENT_PANICKED: &str = "worker_panicked";
const EVENT_POLL_FAILED: &str = "lease_read_failed";
const EVENT_LEASE_ERROR: &str = "lease_write_failed";

/// Runs workers to the assigned count until leasing stops, then waits for every
/// lease in flight.
pub(crate) async fn serve(lessee: Arc<Lessee>, mut assignment: watch::Receiver<Assignment>) {
    let (turns, coordinator) = FleetTurns::start();
    let coordinator = tokio::spawn(coordinator);
    let mut pool = Pool {
        lessee: Arc::clone(&lessee),
        turns,
        assignment: assignment.clone(),
        workers: JoinSet::new(),
        numbers: HashMap::new(),
    };
    let mut spawned = 0;
    loop {
        let wanted = assignment.borrow_and_update().workers;
        for number in spawned..wanted {
            pool.spawn(number);
        }
        spawned = spawned.max(wanted);
        tokio::select! {
            () = lessee.halt.leasing().cancelled() => break,
            changed = assignment.changed() => if changed.is_err() { break },
            Some(joined) = pool.workers.join_next_with_id() => pool.reap(joined),
        }
    }
    while let Some(joined) = pool.workers.join_next_with_id().await {
        pool.reap(joined);
    }
    drop(pool);
    drop(coordinator.await);
}

/// The workers, and which number each task runs under.
struct Pool {
    lessee: Arc<Lessee>,
    turns: FleetTurns,
    assignment: watch::Receiver<Assignment>,
    workers: JoinSet<()>,
    numbers: HashMap<Id, u32>,
}

impl Pool {
    fn spawn(&mut self, number: u32) {
        let worker = Worker {
            number,
            lessee: Arc::clone(&self.lessee),
            turns: self.turns.clone(),
            assignment: self.assignment.clone(),
            failures: endless(),
        };
        let task = self.workers.spawn(worker.run());
        self.numbers.insert(task.id(), number);
    }

    /// Accounts for a worker that ended, and restarts one that panicked while
    /// leasing goes on.
    fn reap(&mut self, joined: Result<(Id, ()), JoinError>) {
        let ended = match &joined {
            Ok((id, ())) => *id,
            Err(failure) => failure.id(),
        };
        let number = self.numbers.remove(&ended);
        if let (Err(failure), Some(number)) = (joined, number)
            && failure.is_panic()
        {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let event = EVENT_PANICKED;
            tracing::error!(
                error_code = code,
                worker = number,
                event,
                "a worker panicked; it starts again"
            );
            if !self.lessee.halt.leasing().is_cancelled() {
                self.spawn(number);
            }
        }
    }
}

/// One worker: wait to be wanted, poll, run, repeat.
struct Worker {
    number: u32,
    lessee: Arc<Lessee>,
    turns: FleetTurns,
    assignment: watch::Receiver<Assignment>,
    failures: ExponentialBackoff,
}

impl Worker {
    async fn run(mut self) {
        let worker = self.number;
        let event = EVENT_STARTED;
        tracing::info!(worker, event);
        let leasing = self.lessee.halt.leasing().clone();
        while let Some(pause) = self.next().await {
            tokio::select! {
                () = leasing.cancelled() => break,
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
        let leasing = self.lessee.halt.leasing();
        tokio::select! {
            () = leasing.cancelled() => return None,
            wanted = self.assignment.wait_for(|now| now.takes_work(number)) => wanted.ok()?,
        };
        let polled = tokio::select! {
            () = leasing.cancelled() => return None,
            polled = self.lessee.plane.lease() => polled,
        };
        let body = match polled {
            Ok(body) => body,
            Err(failure) if self.lessee.halt.stops_on(&failure) => return None,
            Err(failure) => return Some(self.failed(&failure, EVENT_POLL_FAILED)),
        };
        let reply = match body.decode::<LeaseResponse<'_>>() {
            Ok(reply) => reply,
            Err(failure) => return Some(self.failed(&failure, EVENT_POLL_FAILED)),
        };
        self.failures = endless();
        let Some(lease) = reply.lease else {
            let hinted = reply.retry_after_ms.unwrap_or(NO_WORK_RETRY_AFTER_MS);
            return Some(Duration::from_millis(u64::from(hinted)).max(MIN_POLL_PAUSE));
        };
        if let Err(failure) = self.lessee.run(&self.turns, &lease).await {
            return Some(self.failed(&failure, EVENT_LEASE_ERROR));
        }
        Some(Duration::ZERO)
    }

    /// Logs a failed poll or lease, and backs off before the next.
    fn failed(&mut self, failure: &crate::Error, event: &'static str) -> Duration {
        let code = failure.code().as_str();
        let worker = self.number;
        tracing::warn!(
            error_code = code,
            worker,
            event,
            "the worker backs off before polling again"
        );
        self.failures.next().unwrap_or(MIN_POLL_PAUSE)
    }
}

#[cfg(test)]
#[path = "worker_pool/tests.rs"]
mod tests;
