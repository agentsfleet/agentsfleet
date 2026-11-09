//! Notifications schedule offers. A separate, replica-wide recovery probe covers lost notifications.

mod recovery;
mod subscriptions;

use crate::{Gateway, Result};
use tokio_util::sync::CancellationToken;

/// Subscription repair, notification handling and recovery keep independent progress.
pub(crate) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    let _cancel_on_exit = stop.clone().drop_guard();
    tokio::try_join!(
        subscriptions::run(gateway.clone(), stop.clone()),
        recovery::run(gateway.clone(), stop.clone()),
        notifications(gateway, stop)
    )?;
    Ok(())
}

async fn notifications(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    let mut generations = gateway.inner.dependencies.wake.generations();
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
    let fleets = gateway.inner.dependencies.wake.fleets();
    let sessions = gateway.inner.registry.candidates(&fleets);
    // A hint identifies a fleet, not a host whose labels can serve it.
    // Every eligible local host must get a chance when earlier hosts select nothing.
    for session in sessions {
        session.request_offer();
    }
}
