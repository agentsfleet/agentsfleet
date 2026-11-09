//! This is the spec's backstop, not runner polling: at most one query per replica per tick.

use crate::{Gateway, Operation, Result, error::within};
use af_core::timing::{OFFER_FALLBACK_TICK, STREAM_ANSWER_TIMEOUT};
use afd_authoring::types::{DraftState, MessageState};
use afd_bus::wake::Wake;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

/// Notifications never reset this timer. A hot fleet cannot postpone lost-work recovery.
pub(super) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    probe_ticks(&gateway.inner.dependencies.wake, &stop, || {
        pending_work(&gateway, &stop)
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

/// The query only asks whether a pass may help; it does not acquire leases or reimplement admission.
async fn pending_work(gateway: &Gateway, stop: &CancellationToken) -> Result<bool> {
    within(
        stop,
        Instant::now() + STREAM_ANSWER_TIMEOUT,
        Operation::Recheck,
        async {
            Ok(sqlx::query_scalar::<_, bool>(include_str!(
                "../../sql/select_waiting_admission.sql"
            ))
            .bind(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE)
            .bind(DraftState::Editing.as_str())
            .bind(MessageState::Queued.as_str())
            .fetch_one(&mut *gateway.inner.dependencies.database.acquire().await?)
            .await?)
        },
    )
    .await
}

#[cfg(test)]
mod tests;
