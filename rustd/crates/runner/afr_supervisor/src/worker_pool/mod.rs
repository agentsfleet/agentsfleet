//! Idle workers await pushed assignments; only an available worker advertises a credit.

use std::{panic::AssertUnwindSafe, sync::Arc};

use af_core::error_code;
use af_wire::{protocol::Down, run::TaggedRun};
use futures_util::FutureExt as _;
use tokio::{
    sync::{Mutex, mpsc, watch},
    task::JoinSet,
};

use crate::{
    Result,
    downlink::{Assigned, policy::Assignment},
    run_loop::Lessee,
    turns::FleetTurns,
};

pub(crate) async fn serve(
    lessee: Arc<Lessee>,
    mut assignment: watch::Receiver<Assignment>,
    assignments: mpsc::Receiver<Assigned>,
) {
    let (turns, coordinator) = FleetTurns::start();
    let coordinator = tokio::spawn(coordinator);
    let assignments = Arc::new(Mutex::new(assignments));
    let mut workers = JoinSet::new();
    let mut spawned = 0;
    loop {
        let policy = assignment.borrow_and_update().clone();
        let Some(workers_wanted) = offer(&lessee, &policy) else {
            break;
        };
        for number in spawned..workers_wanted {
            workers.spawn(
                Worker {
                    number,
                    lessee: Arc::clone(&lessee),
                    turns: turns.clone(),
                    assignment: assignment.clone(),
                    assignments: Arc::clone(&assignments),
                }
                .run(),
            );
        }
        spawned = spawned.max(workers_wanted);
        tokio::select! {
            () = lessee.halt.taking_runs().cancelled() => break,
            changed = assignment.changed() => if changed.is_err() { break; },
        }
    }
    finish_pool(&lessee, turns, coordinator, &mut workers).await;
}

/// The workers `policy` wants, offered as credits within the engine's memory
/// bound. Only an assignment that takes work names the network its runs get,
/// so only that one tells the engine. None, the runner halted, when the count
/// does not fit.
fn offer(lessee: &Lessee, policy: &Assignment) -> Option<u32> {
    let workers_wanted = if policy.takes_work(0) {
        lessee.engine.network_assigned(policy.egress.kind());
        policy.workers
    } else {
        0
    };
    let wanted = match usize::try_from(workers_wanted) {
        Ok(wanted) => wanted,
        Err(source) => {
            let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::error!(event = "runner_worker_capacity_refused", error_code, error = %source);
            lessee.halt.stop();
            return None;
        }
    };
    lessee.holds.resize(wanted);
    lessee
        .plane
        .stream
        .running
        .capacity(wanted, lessee.engine.admission_bound());
    Some(workers_wanted)
}

struct Worker {
    number: u32,
    lessee: Arc<Lessee>,
    turns: FleetTurns,
    assignment: watch::Receiver<Assignment>,
    assignments: Arc<Mutex<mpsc::Receiver<Assigned>>>,
}

impl Worker {
    async fn run(mut self) {
        let worker = self.number;
        tracing::info!(event = "worker_started", worker);
        while !self.lessee.halt.taking_runs().is_cancelled() {
            match AssertUnwindSafe(self.next()).catch_unwind().await {
                Ok(Ok(true)) => {}
                Ok(Ok(false)) => break,
                Ok(Err(error)) => {
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "run_write_failed", worker, error_code);
                    if self.lessee.halt.stops_on(&error) {
                        break;
                    }
                }
                Err(_panic) => {
                    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                    tracing::error!(event = "worker_panicked", worker, error_code);
                }
            }
        }
        tracing::info!(event = "worker_stopped", worker);
    }

    async fn next(&mut self) -> Result<bool> {
        let number = self.number;
        let taking_runs = self.lessee.halt.taking_runs().clone();
        while !self.assignment.borrow_and_update().takes_work(number) {
            tokio::select! {
                () = taking_runs.cancelled() => return Ok(false),
                changed = self.assignment.changed() => if changed.is_err() { return Ok(false); },
            }
        }
        let assigned = tokio::select! {
            () = taking_runs.cancelled() => return Ok(false),
            changed = self.assignment.changed() => return Ok(changed.is_ok()),
            assigned = async { self.assignments.lock().await.recv().await } => assigned,
        };
        let Some(assigned) = assigned else {
            return Ok(false);
        };
        if assigned.session.is_cancelled() || assigned.registration.stop.is_cancelled() {
            return Ok(true);
        }
        let egress = self.assignment.borrow().egress.clone();
        let Down::Run { run } = assigned.frame.decode::<Down<'_>>()? else {
            return Err(crate::error::ErrorKind::StreamProtocol.into());
        };
        let TaggedRun::Fleet(run) = run;
        self.lessee
            .run(
                &self.turns,
                &run,
                &egress,
                assigned.registration.stop.clone(),
            )
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;

async fn finish_pool(
    lessee: &Lessee,
    turns: FleetTurns,
    coordinator: tokio::task::JoinHandle<()>,
    workers: &mut JoinSet<()>,
) {
    lessee.plane.stream.running.capacity(0, None);
    crate::shutdown::workers(&lessee.halt, workers).await;
    drop(turns);
    if let Err(error) = coordinator.await {
        let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::error!(event = "fleet_turn_coordinator_failed", error_code, error = %error);
    }
}
