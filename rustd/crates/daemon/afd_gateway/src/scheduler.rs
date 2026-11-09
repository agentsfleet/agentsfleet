//! Notifications schedule offers. A separate, replica-wide recovery probe covers lost notifications.

mod recovery;
mod subscriptions;

use std::collections::HashSet;
use tokio_util::sync::CancellationToken;
use crate::{Gateway, Result};

/// Subscription repair, notification handling and recovery keep independent progress.
pub(crate) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    let _cancel_on_exit = stop.clone().drop_guard();
    tokio::try_join!(subscriptions::run(gateway.clone(), stop.clone()),
        recovery::run(gateway.clone(), stop.clone()), notifications(gateway, stop))?;
    Ok(())
}

async fn notifications(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    let mut generations = gateway.0.dependencies.wake.generations();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            changed = generations.changed() => {
                if changed.is_err() { return Ok(()); }
                request_offers(&gateway);
            }
        }
    }
}

/// Empty fleet membership means an unknown gap; all eligible sessions get a recovery opportunity.
fn request_offers(gateway: &Gateway) {
    let mut fleets = HashSet::new();
    while let Some(fleet) = gateway.0.dependencies.wake.take() { fleets.insert(fleet); }
    let sessions = gateway.0.registry.candidates(&fleets);
    let limit = if fleets.is_empty() { sessions.len() } else { fleets.len() };
    for session in sessions.into_iter().take(limit) { session.request_offer(); }
}
