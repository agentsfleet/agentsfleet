//! Notifications schedule assignments; no timer probes for work a lost notification left waiting.

mod lapse;
mod recovery;
mod subscriptions;

pub(crate) use lapse::{deadline, recheck_at};
pub(crate) use subscriptions::REPAIR_BACKOFF_MAX;

use crate::{
    Gateway, Result,
    connection::{AssignMode, AssignOutcome, ConnectionHandle},
    registry::Registry,
};
use af_core::id::Uuid7;
use afd_bus::wake::Wake;
use std::{collections::HashSet, sync::Arc};
use tokio_util::sync::CancellationToken;

/// Subscription repair, notification handling and the boot read keep
/// independent progress. None fails: a lost broker is repaired, a failed probe
/// or pass is observed and the next wake tries again, and a failed boot read is
/// observed once.
pub(crate) async fn run(gateway: Gateway, stop: CancellationToken) {
    let _cancel_on_exit = stop.clone().drop_guard();
    tokio::join!(
        lapse::outstanding(&gateway, &stop),
        subscriptions::run(gateway.clone(), stop.clone()),
        notifications(
            &gateway.inner.registry,
            &gateway.inner.dependencies.wake,
            gateway
                .inner
                .dependencies
                .recovery
                .max_concurrent_assignments
                .get(),
            stop.clone(),
            |runners| {
                let (gateway, stop) = (&gateway, &stop);
                async move {
                    let waiting = recovery::waiting_work(gateway, stop, &runners).await?;
                    Ok(if waiting.pending() {
                        Probe::Waiting(waiting.idle().iter().cloned().collect())
                    } else {
                        Probe::Nothing
                    })
                }
            },
        )
    );
}

/// One ordinary probe can skip every tenant runner with no eligible work.
#[derive(Debug)]
pub(crate) enum Probe {
    /// No connection can take pending work.
    Nothing,
    /// Work waits; these tenant runners cannot take it.
    Waiting(HashSet<Uuid7>),
}

async fn notifications<F: Future<Output = Result<Probe>>>(
    registry: &Registry,
    wake: &Wake,
    assignments: usize,
    stop: CancellationToken,
    pending: impl FnMut(Vec<Uuid7>) -> F,
) {
    tokio::join!(
        ordinary_notifications(registry, wake, &stop, pending),
        recovery::notifications(registry, wake, assignments, &stop),
    );
}

async fn ordinary_notifications<F: Future<Output = Result<Probe>>>(
    registry: &Registry,
    wake: &Wake,
    stop: &CancellationToken,
    mut pending: impl FnMut(Vec<Uuid7>) -> F,
) {
    let mut generations = wake.generations();
    let mut observed = generations.borrow_and_update().rings();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => return,
            changed = generations.changed() => {
                if changed.is_err() { return; }
            }
        }
        let current = generations.borrow_and_update().rings();
        let budget = current.wrapping_sub(observed);
        observed = current;
        if budget == 0 {
            continue;
        }
        let idle = match pending(registry.runners()).await {
            Ok(Probe::Waiting(runners)) => runners,
            Ok(Probe::Nothing) => continue,
            Err(error) => {
                Gateway::observe(&error, crate::Operation::Assign);
                HashSet::new()
            }
        };
        let connections = registry.candidates(&wake.fleets(), &idle);
        tokio::select! {
            biased;
            () = stop.cancelled() => return,
            () = pass(connections, budget) => {}
        }
    }
}

/// Empty assignments advance the walk; successful ones spend the pass's run budget.
/// A session's fault, such as a fleet whose gates cannot be read, is logged and
/// ends only that session's turn in the walk.
async fn pass(connections: Vec<Arc<ConnectionHandle>>, mut budget: u64) {
    for connection in connections {
        while budget > 0 {
            match connection.assign(AssignMode::Ordinary).await {
                Ok(AssignOutcome::Issued) => budget -= 1,
                Ok(_) => break,
                Err(error) => {
                    Gateway::observe(&error, crate::Operation::Assign);
                    break;
                }
            }
        }
        if budget == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
