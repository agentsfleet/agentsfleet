//! Notifications schedule offers. A separate, replica-wide recovery probe covers lost notifications.

mod recovery;
mod subscriptions;

use crate::{Gateway, Result, registry::Registry, session::SessionHandle};
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
            stop.clone(),
            || recovery::pending_work(&gateway, &stop),
        )
    )?;
    Ok(())
}

/// A wake for work no session can claim, such as a message to a fleet already
/// running, would cost one discovery scan per ready session. One `pending` probe
/// per pass answers it first; a recovery generation has already probed.
async fn notifications<F: Future<Output = Result<bool>>>(
    registry: &Registry,
    wake: &Wake,
    stop: CancellationToken,
    mut pending: impl FnMut() -> F,
) -> Result<()> {
    let mut generations = wake.generations();
    let mut observed = *generations.borrow_and_update();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            changed = generations.changed() => {
                if changed.is_err() { return Ok(()); }
            }
        }
        let current = *generations.borrow_and_update();
        let budget = current.wrapping_sub(observed).max(1);
        observed = current;
        if wake.recovery().is_none() {
            match pending().await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => Gateway::observe(&error, crate::Operation::Offer),
            }
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
            if !session.claim().await? {
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
