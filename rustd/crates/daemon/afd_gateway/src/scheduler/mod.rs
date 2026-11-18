//! Notifications schedule offers. A separate, replica-wide recovery probe covers lost notifications.

mod recovery;
mod subscriptions;

use crate::{Gateway, Result, registry::Registry, session::SessionHandle};
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

/// What one probe says about a pass.
#[derive(Debug)]
pub(crate) enum Probe {
    /// No session can claim anything; the pass is skipped.
    Nothing,
    /// Work waits. These tenant runners have none of it to take, because a
    /// tenant runner claims only its tenant's work, so the pass skips them.
    Waiting(HashSet<Uuid7>),
}

/// A wake for work no session can claim, such as a message to a fleet already
/// running, would cost one discovery scan per ready session. One `pending` probe
/// per pass answers it first, and names the tenant runners the waiting work
/// cannot reach; a recovery generation has already probed and scans them all.
async fn notifications<F: Future<Output = Result<Probe>>>(
    registry: &Registry,
    wake: &Wake,
    stop: CancellationToken,
    mut pending: impl FnMut(Vec<Uuid7>) -> F,
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
        let mut idle = HashSet::new();
        if wake.recovery().is_none() {
            match pending(registry.runners()).await {
                Ok(Probe::Waiting(runners)) => idle = runners,
                Ok(Probe::Nothing) => continue,
                Err(error) => Gateway::observe(&error, crate::Operation::Offer),
            }
        }
        let sessions = registry.candidates(&wake.fleets(), &idle);
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
