//! This is the spec's backstop, not runner polling: at most one query per replica per tick.

mod drain;

use crate::registry::Registry;
use crate::{Gateway, Operation, Result, error::within};
use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use afd_bus::wake::Wake;
use afd_fleet::run::waiting::{Waiting, waiting};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

pub(super) async fn notifications(
    registry: &Registry,
    wake: &Wake,
    claims: usize,
    stop: &CancellationToken,
) -> Result<()> {
    let mut generations = wake.generations();
    loop {
        let observed = generations.borrow_and_update().recovery();
        if let Some(recovery) = observed {
            registry.request_recovery(recovery);
            let credited = registry.candidates(
                &std::collections::HashSet::new(),
                &std::collections::HashSet::new(),
            );
            drain::drain(credited, claims, stop).await;
            wake.recovered(recovery);
        }
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            changed = generations.changed() => if changed.is_err() { return Ok(()); },
        }
    }
}

/// Notifications never reset this timer. A hot fleet cannot postpone lost-work recovery.
pub(super) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    probe_ticks(
        &gateway.inner.dependencies.wake,
        &stop,
        gateway.inner.dependencies.recovery.assign_fallback,
        || async { Ok(waiting_work(&gateway, &stop, &[]).await?.pending()) },
    )
    .await;
    Ok(())
}

async fn probe_ticks<F: Future<Output = Result<bool>>>(
    wake: &Wake,
    stop: &CancellationToken,
    interval: std::time::Duration,
    mut probe: impl FnMut() -> F,
) {
    let mut tick = tokio::time::interval_at(Instant::now() + interval, interval);
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
/// cannot help; it does not acquire runs or reimplement admission.
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
            let mut connection_id = gateway.inner.dependencies.database.acquire().await?;
            Ok(waiting(
                &mut connection_id,
                runners,
                gateway
                    .inner
                    .dependencies
                    .runs
                    .runs
                    .checkpoint_resume_enabled(),
            )
            .await?)
        },
    )
    .await
}

#[cfg(test)]
mod tests;
