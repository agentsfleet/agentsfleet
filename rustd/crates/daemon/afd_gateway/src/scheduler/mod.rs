//! Notifications schedule offers. A separate, replica-wide recovery probe covers lost notifications.

mod recovery;
mod subscriptions;

use crate::{
    Gateway, Result,
    registry::Registry,
    session::{OfferMode, OfferOutcome, SessionHandle},
};
use afd_bus::wake::Wake;
use std::sync::Arc;
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
            stop
        )
    )?;
    Ok(())
}

async fn notifications(
    registry: &Registry,
    wake: &Wake,
    claims: usize,
    stop: CancellationToken,
) -> Result<()> {
    tokio::try_join!(
        ordinary_notifications(registry, wake, &stop),
        recovery::notifications(registry, wake, claims, &stop),
    )?;
    Ok(())
}

async fn ordinary_notifications(
    registry: &Registry,
    wake: &Wake,
    stop: &CancellationToken,
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
        let sessions = registry.candidates(&wake.fleets());
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            result = pass(sessions, budget) => {
                if let Err(error) = result {
                    Gateway::observe(&error, crate::Operation::Offer);
                }
            }
        }
    }
}

/// Empty claims advance the walk; successful claims spend the pass's lease budget.
async fn pass(sessions: Vec<Arc<SessionHandle>>, mut budget: u64) -> Result<()> {
    for session in sessions {
        while budget > 0 {
            if session.claim(OfferMode::Ordinary).await? != OfferOutcome::Issued {
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
