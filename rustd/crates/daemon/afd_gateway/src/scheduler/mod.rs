//! Notifications schedule offers. A separate, replica-wide recovery probe covers lost notifications.

mod recovery;
mod subscriptions;

use crate::{
    Gateway, Result,
    connection::{ConnectionHandle, OfferMode, OfferOutcome},
    registry::Registry,
};
use af_core::id::Uuid7;
use afd_bus::wake::Wake;
use std::{collections::HashSet, sync::Arc};
use tokio_util::sync::CancellationToken;

/// Subscription repair, notification handling and recovery keep independent progress.
pub(crate) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    let _cancel_on_exit = stop.clone().drop_guard();
    tokio::try_join!(
        subscriptions::run(gateway.clone(), stop.clone()),
        recovery::run(gateway.clone(), stop.clone()),
        notifications(
            &gateway.inner.registry,
            &gateway.inner.dependencies.wake,
            gateway
                .inner
                .dependencies
                .recovery
                .max_concurrent_claims
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
    )?;
    Ok(())
}

/// One ordinary probe can skip every tenant runner with no eligible work.
#[derive(Debug)]
pub(crate) enum Probe {
    /// No connection can claim pending work.
    Nothing,
    /// Work waits; these tenant runners cannot claim it.
    Waiting(HashSet<Uuid7>),
}

async fn notifications<F: Future<Output = Result<Probe>>>(
    registry: &Registry,
    wake: &Wake,
    claims: usize,
    stop: CancellationToken,
    pending: impl FnMut(Vec<Uuid7>) -> F,
) -> Result<()> {
    tokio::try_join!(
        ordinary_notifications(registry, wake, &stop, pending),
        recovery::notifications(registry, wake, claims, &stop),
    )?;
    Ok(())
}

async fn ordinary_notifications<F: Future<Output = Result<Probe>>>(
    registry: &Registry,
    wake: &Wake,
    stop: &CancellationToken,
    mut pending: impl FnMut(Vec<Uuid7>) -> F,
) -> Result<()> {
    let mut generations = wake.generations();
    let mut observed = generations.borrow_and_update().rings();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            changed = generations.changed() => {
                if changed.is_err() { return Ok(()); }
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
            () = stop.cancelled() => return Ok(()),
            result = pass(connections, budget) => {
                if let Err(error) = result {
                    Gateway::observe(&error, crate::Operation::Assign);
                }
            }
        }
    }
}

/// Empty claims advance the walk; successful claims spend the pass's run budget.
async fn pass(connections: Vec<Arc<ConnectionHandle>>, mut budget: u64) -> Result<()> {
    for connection in connections {
        while budget > 0 {
            if connection.claim(OfferMode::Ordinary).await? != OfferOutcome::Issued {
                break;
            }
            budget -= 1;
        }
        if budget == 0 {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
