//! Idle workers await pushed offers; only an available worker advertises a credit.

use std::{panic::AssertUnwindSafe, sync::Arc};

use af_core::error_code;
use af_wire::{lease::TaggedLease, protocol::Down};
use futures_util::FutureExt as _;
use tokio::{
    sync::{Mutex, mpsc, watch},
    task::JoinSet,
};

use crate::{
    Result,
    downlink::{Offered, policy::Assignment},
    lease_loop::Lessee,
    turns::FleetTurns,
};

pub(crate) async fn serve(
    lessee: Arc<Lessee>,
    mut assignment: watch::Receiver<Assignment>,
    offers: mpsc::Receiver<Offered>,
) {
    let (turns, coordinator) = FleetTurns::start();
    let coordinator = tokio::spawn(coordinator);
    let offers = Arc::new(Mutex::new(offers));
    let mut workers = JoinSet::new();
    let mut spawned = 0;
    loop {
        let policy = assignment.borrow_and_update().clone();
        let workers_wanted = if policy.takes_work(0) {
            policy.workers
        } else {
            0
        };
        let wanted = usize::try_from(workers_wanted);
        let wanted = match wanted {
            Ok(wanted) => wanted,
            Err(source) => {
                let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::error!(event = "runner_worker_capacity_refused", error_code, error = %source);
                lessee.halt.stop();
                break;
            }
        };
        lessee.holds.resize(wanted);
        lessee.plane.stream.running.capacity(wanted);
        for number in spawned..workers_wanted {
            workers.spawn(
                Worker {
                    number,
                    lessee: Arc::clone(&lessee),
                    turns: turns.clone(),
                    assignment: assignment.clone(),
                    offers: Arc::clone(&offers),
                }
                .run(),
            );
        }
        spawned = spawned.max(workers_wanted);
        tokio::select! {
            () = lessee.halt.leasing().cancelled() => break,
            changed = assignment.changed() => if changed.is_err() { break; },
        }
    }
    lessee.plane.stream.running.capacity(0);
    while let Some(done) = workers.join_next().await {
        if let Err(error) = done {
            let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::error!(event = "worker_panicked", error_code, error = %error);
        }
    }
    drop(turns);
    if let Err(error) = coordinator.await {
        let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::error!(event = "fleet_turn_coordinator_failed", error_code, error = %error);
    }
}

struct Worker {
    number: u32,
    lessee: Arc<Lessee>,
    turns: FleetTurns,
    assignment: watch::Receiver<Assignment>,
    offers: Arc<Mutex<mpsc::Receiver<Offered>>>,
}

impl Worker {
    async fn run(mut self) {
        let worker = self.number;
        tracing::info!(event = "worker_started", worker);
        while !self.lessee.halt.leasing().is_cancelled() {
            match AssertUnwindSafe(self.next()).catch_unwind().await {
                Ok(Ok(true)) => {}
                Ok(Ok(false)) => break,
                Ok(Err(error)) => {
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "lease_write_failed", worker, error_code);
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
        let leasing = self.lessee.halt.leasing().clone();
        while !self.assignment.borrow_and_update().takes_work(number) {
            tokio::select! {
                () = leasing.cancelled() => return Ok(false),
                changed = self.assignment.changed() => if changed.is_err() { return Ok(false); },
            }
        }
        let offer = tokio::select! {
            () = leasing.cancelled() => return Ok(false),
            changed = self.assignment.changed() => return Ok(changed.is_ok()),
            offer = async { self.offers.lock().await.recv().await } => offer,
        };
        let Some(offer) = offer else {
            return Ok(false);
        };
        if offer.session.is_cancelled() || offer.registration.stop.is_cancelled() {
            return Ok(true);
        }
        let egress = self.assignment.borrow().egress.clone();
        let Down::Offer { lease } = offer.frame.decode::<Down<'_>>()? else {
            return Err(crate::error::ErrorKind::StreamProtocol.into());
        };
        match lease {
            TaggedLease::Fleet(lease) => {
                self.lessee
                    .run(
                        &self.turns,
                        &lease,
                        &egress,
                        offer.registration.stop.clone(),
                    )
                    .await?;
            }
            TaggedLease::Authoring(lease) => {
                self.lessee
                    .author(&lease, offer.registration.stop.clone())
                    .await?;
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
