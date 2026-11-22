//! Recovery generations from the bus drain credited hosts; nothing here runs on a timer.

mod drain;

use crate::registry::Registry;
use crate::{Gateway, Operation, Result, error::within};
use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use afd_bus::wake::Wake;
use afd_fleet::run::waiting::{Waiting, waiting};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub(super) async fn notifications(
    registry: &Registry,
    wake: &Wake,
    assignments: usize,
    stop: &CancellationToken,
) {
    let mut generations = wake.generations();
    loop {
        let observed = generations.borrow_and_update().recovery();
        if let Some(recovery) = observed {
            registry.request_recovery(recovery);
            let credited = registry.candidates(
                &std::collections::HashSet::new(),
                &std::collections::HashSet::new(),
            );
            drain::drain(credited, assignments, stop).await;
            wake.recovered(recovery);
        }
        tokio::select! {
            biased;
            () = stop.cancelled() => return,
            changed = generations.changed() => if changed.is_err() { return; },
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
