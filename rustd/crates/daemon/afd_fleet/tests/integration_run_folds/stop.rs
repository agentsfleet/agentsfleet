//! A fold retry against a Stop of one of its folds. Both write the run's folds; the retry
//! takes the thread first, as Stop does, so the two queue on it instead of taking two
//! folds in opposite orders.

use afd_turns::Producer;
use afd_turns::view::stop::{Stop, hold_thread, stop_on};
use sqlx::Connection as _;

use super::support::{Checked, STEER_JSON, STEER_TEXT, assign_as_holder, send, workspace_of};
use crate::report_seed::held;

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// Returns once some backend waits on a lock `pid` holds.
async fn wait_for_a_waiter(database: &afd_db::Db, pid: i32) -> Checked {
    let mut probe = database.acquire().await?;
    loop {
        let queued: bool = sqlx::query_scalar(BLOCKING)
            .bind(pid)
            .fetch_one(&mut *probe)
            .await?;
        if queued {
            return Ok(());
        }
        tokio::task::yield_now().await;
    }
}

/// A Stop holds the thread; the run's fold retry queues behind it; the Stop settles one
/// fold and, through the run, every turn the run serves, then commits. The retry then
/// finds nothing left to assign, and neither side is chosen as a deadlock victim.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fold_retry_waits_for_a_stop_holding_the_thread() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let first = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let second = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [first.assigned(STEER_TEXT), second.assigned(STEER_TEXT)]
    );

    let mut holder = held.fixtures.database.acquire().await?;
    let mut stopping = holder.begin().await?;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *stopping)
        .await?;
    hold_thread(&mut stopping, &workspace, &held.fleet).await?;

    let mut retry = Box::pin(assign_as_holder(&held, 0));
    let waited = tokio::select! {
        _ = &mut retry => false,
        queued = wait_for_a_waiter(&held.fixtures.database, pid) => { queued?; true }
    };
    assert!(
        waited,
        "the retry queues behind the Stop holding the thread"
    );

    let stopped = stop_on(&mut stopping, &workspace, &held.fleet, &first.id)
        .await?
        .ok_or("the fold exists")?;
    stopping.commit().await?;
    assert!(
        matches!(
            stopped.stop,
            Stop::Cancelled {
                running: true,
                turns: 3,
                ..
            }
        ),
        "Stop cancels the fold, the run's head and the other fold: {:?}",
        stopped.stop
    );
    let retried = (&mut retry).await?;
    drop(retry);
    assert!(retried.is_empty(), "every fold was settled: {retried:?}");
    held.fixtures.cleanup().await;
    Ok(())
}
