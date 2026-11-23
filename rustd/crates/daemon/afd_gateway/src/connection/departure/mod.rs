//! A connection that ends with no successor and no replica drain schedules two
//! one-shot effects for the runner it served; neither repeats.
//!
//! The runs it held lapse at their deadlines, and the candidate reads judge a
//! lapse when they run, so one pass at the latest deadline reissues every turn
//! it left. Its silence is judged once, at its last beat plus the offline
//! window. A runner that came back and renewed or beat leaves both with nothing
//! to do, so neither is cancelled. A replica drain ends both; the next boot's
//! read covers the pass.

use af_core::{clock, id::Uuid7};
use tokio::time::Instant;

use super::{ConnectionHandle, cache::RunCache};
use crate::{Gateway, Operation, Result, scheduler};

/// Called once the connection's jobs have stopped, so no renewal can still
/// move the deadlines `cache` holds.
pub(super) fn depart(gateway: &Gateway, handle: &ConnectionHandle, cache: &RunCache) {
    // A successor owns the runner, or the replica is stopping.
    if handle.is_superseded() || handle.drain.is_cancelled() {
        return;
    }
    let dependencies = &gateway.inner.dependencies;
    let lapse = cache
        .last_lapse(dependencies.wall_clock.now())
        .map(|at| scheduler::deadline(dependencies, at));
    let runner = handle.runner.clone();
    let draining = gateway.inner.registry.draining();
    let owner = gateway.clone();
    gateway.inner.tasks.spawn(async move {
        let scheduled = async {
            tokio::join!(lapsed(&owner, lapse), silent(&owner, &runner));
        };
        tokio::select! {
            biased;
            () = draining.cancelled() => {}
            () = scheduled => {}
        }
    });
}

async fn lapsed(gateway: &Gateway, lapse: Option<Instant>) {
    if let Some(deadline) = lapse {
        scheduler::recheck_at(&gateway.inner.dependencies.wake, deadline).await;
    }
}

async fn silent(gateway: &Gateway, runner: &Uuid7) {
    if let Err(error) = visit(gateway, runner).await {
        Gateway::observe(&error, Operation::Heartbeat);
    }
}

/// The beat is read now and the visit compares against it, so a beat that
/// lands in between leaves the runner untouched.
async fn visit(gateway: &Gateway, runner: &Uuid7) -> Result<()> {
    let dependencies = &gateway.inner.dependencies;
    let Some(seen) = dependencies.runners.last_seen(runner).await? else {
        return Ok(());
    };
    let offline = clock::after(seen, dependencies.runners.offline_after());
    tokio::time::sleep_until(scheduler::deadline(dependencies, offline)).await;
    dependencies
        .runners
        .record_silence(runner, seen, dependencies.wall_clock.now())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
