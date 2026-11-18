//! This is the spec's backstop, not runner polling: at most one query per replica per tick.

use crate::{Gateway, Operation, Result, error::within};
use af_core::{
    id::Uuid7,
    timing::{OFFER_FALLBACK_TICK, STREAM_ANSWER_TIMEOUT},
};
use afd_bus::wake::Wake;
use afd_fleet::lease::waiting::{Waiting, waiting};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

/// Notifications never reset this timer. A hot fleet cannot postpone lost-work recovery.
pub(super) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    probe_ticks(&gateway.inner.dependencies.wake, &stop, || async {
        Ok(waiting_work(&gateway, &stop, &[]).await?.pending())
    })
    .await;
    Ok(())
}

async fn probe_ticks<F: Future<Output = Result<bool>>>(
    wake: &Wake,
    stop: &CancellationToken,
    mut probe: impl FnMut() -> F,
) {
    let mut tick =
        tokio::time::interval_at(Instant::now() + OFFER_FALLBACK_TICK, OFFER_FALLBACK_TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => return,
            _instant = tick.tick() => {
                match probe().await {
                    Ok(true) => wake.recheck(),
                    Err(error) if !stop.is_cancelled() => crate::Gateway::observe(&error, Operation::Recheck),
                    Ok(false) | Err(_) => {}
                }
            }
        }
    }
}

/// The query only asks whether a pass may help, and which of `runners` it
/// cannot help; it does not acquire leases or reimplement admission.
pub(super) async fn waiting_work(
    gateway: &Gateway,
    stop: &CancellationToken,
    runners: &[Uuid7],
) -> Result<Waiting> {
    within(
        stop,
        Instant::now() + STREAM_ANSWER_TIMEOUT,
        Operation::Recheck,
        async {
            let mut connection = gateway.inner.dependencies.database.acquire().await?;
            Ok(waiting(&mut connection, runners).await?)
        },
    )
    .await
}

#[cfg(test)]
mod tests;
