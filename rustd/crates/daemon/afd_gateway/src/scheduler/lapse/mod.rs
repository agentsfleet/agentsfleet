//! A lapse instant requests one pass; nothing here repeats.
//!
//! A run's deadline passing is not an event anyone publishes, and the
//! candidate reads judge a lapse only when they run. The two moments a replica
//! knows a deadline it is owed a pass at are a connection's end, for the runs
//! that connection held, and its own boot, for the runs a connection on the
//! previous process held.

use af_core::clock;
use afd_bus::wake::Wake;
use jiff::Timestamp;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::{Gateway, GatewayDependencies, Operation, Result};

/// The monotonic instant the wall-clock `at` falls at, read when called, so
/// the event that learned `at` fixes the deadline rather than a task's first
/// poll. An instant already past is now.
pub(crate) fn deadline(dependencies: &GatewayDependencies, at: Timestamp) -> Instant {
    Instant::now() + clock::elapsed(dependencies.wall_clock.now(), at)
}

/// Sleeps until `deadline`, then requests one full pass.
pub(crate) async fn recheck_at(wake: &Wake, deadline: Instant) {
    tokio::time::sleep_until(deadline).await;
    wake.recheck();
}

/// One read at boot, one sleep to its answer, one pass; `stop` ends the wait.
pub(super) async fn outstanding(gateway: &Gateway, stop: &CancellationToken) {
    let dependencies = &gateway.inner.dependencies;
    boot(dependencies, stop, latest(&dependencies.database)).await;
}

async fn boot(
    dependencies: &GatewayDependencies,
    stop: &CancellationToken,
    read: impl Future<Output = Result<Option<Timestamp>>>,
) {
    let once = async {
        match read.await {
            Ok(Some(at)) => recheck_at(&dependencies.wake, deadline(dependencies, at)).await,
            Ok(None) => {}
            Err(error) => Gateway::observe(&error, Operation::Recheck),
        }
    };
    tokio::select! {
        biased;
        () = stop.cancelled() => {}
        () = once => {}
    }
}

async fn latest(database: &afd_db::Db) -> Result<Option<Timestamp>> {
    let at: Option<jiff_sqlx::Timestamp> =
        sqlx::query_scalar(include_str!("select_outstanding_run_expiry.sql"))
            .bind(afd_state::sql::RUN_STATUS_ACTIVE)
            .fetch_one(&mut *database.acquire().await?)
            .await?;
    Ok(at.map(jiff_sqlx::Timestamp::to_jiff))
}

#[cfg(test)]
mod tests;
