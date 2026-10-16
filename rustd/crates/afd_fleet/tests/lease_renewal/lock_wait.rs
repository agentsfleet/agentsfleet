//! The original deadline must still be live after a renewal waits for row locks.

use super::{expiry::snapshot, fixture::Running};
use afd_fleet::lease::Renewed;
use sqlx::{Connection as _, Postgres, Transaction};
use std::time::Duration;

const LOCK: &str =
    "SELECT pg_backend_pid() FROM fleet.runner_leases WHERE id = $1::uuid FOR UPDATE";
const BLOCKED: &str =
    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";
const EXPIRE: &str = const_format::concatcp!(
    "UPDATE fleet.runner_leases SET lease_expires_at = \
    ",
    afd_db::statement::NOW_MS,
    " WHERE id = $1::uuid",
);
const WAIT_LIMIT: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_renewal_rechecks_expiry_after_waiting_for_a_lease_lock() {
    let running = Running::create().await;
    let before = snapshot(&running).await;
    let mut connection = running
        .fixtures
        .database
        .acquire()
        .await
        .expect("hold lease row");
    let mut held = connection.begin().await.expect("open blocker");
    let blocker: i32 = sqlx::query_scalar(LOCK)
        .bind(running.lease.as_str())
        .fetch_one(&mut *held)
        .await
        .expect("lock the original live lease");
    let (verdict, ()) = tokio::join!(
        running.renew_from_old_request(),
        expire_while_blocked(&running, held, blocker)
    );
    assert_eq!(verdict, Renewed::Lost);
    let after = snapshot(&running).await;
    assert!(after.0 < before.0, "the injected deadline stays expired");
    assert_eq!(after.1, before.1, "a refusal cannot advance metering");
    assert_eq!(after.2, before.2, "a refusal cannot advance affinity");
    drop(connection);
    running.cleanup().await;
}

async fn expire_while_blocked(
    running: &Running,
    mut held: Transaction<'_, Postgres>,
    blocker: i32,
) {
    tokio::time::timeout(WAIT_LIMIT, wait_for_blocked(running, blocker))
        .await
        .expect("renewal reaches the held row");
    sqlx::query(EXPIRE)
        .bind(running.lease.as_str())
        .execute(&mut *held)
        .await
        .expect("expire the lease while renewal waits");
    held.commit().await.expect("release the expired row");
}

async fn wait_for_blocked(running: &Running, blocker: i32) {
    let mut observer = running
        .fixtures
        .database
        .acquire()
        .await
        .expect("observe row wait");
    loop {
        if sqlx::query_scalar::<_, bool>(BLOCKED)
            .bind(blocker)
            .fetch_one(&mut *observer)
            .await
            .expect("inspect blocking relation")
        {
            return;
        }
        tokio::task::yield_now().await;
    }
}
