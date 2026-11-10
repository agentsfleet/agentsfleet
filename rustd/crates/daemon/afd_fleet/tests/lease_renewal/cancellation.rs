//! A missed cancel notification cannot renew either execution deadline.

use super::{expiry::snapshot, fixture::Running};
use afd_fleet::lease::Renewed;
use sqlx::Connection as _;

type TestResult<T = ()> = afd_fleet::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_cancel_whose_push_is_lost_ends_at_the_next_renewal() -> TestResult {
    let running = Running::create().await;
    let before = snapshot(&running).await;
    sqlx::query("UPDATE runner.leases SET cancel_requested_at = clock_timestamp() WHERE id = $1")
        .bind(&running.lease)
        .execute(&mut *running.fixtures.database.acquire().await?)
        .await?;
    assert_eq!(running.renew_from_old_request().await, Renewed::Cancelled);
    assert_eq!(snapshot(&running).await, before);
    running.cleanup().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_cancel_committed_while_renewal_waits_cannot_be_overwritten() -> TestResult {
    let running = Running::create().await;
    let before = snapshot(&running).await;
    let mut connection = running.fixtures.database.acquire().await?;
    let mut transaction = connection.begin().await?;
    let blocker: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM runner.leases WHERE id = $1 FOR UPDATE")
            .bind(&running.lease)
            .fetch_one(&mut *transaction)
            .await?;
    let cancelling = async {
        wait_for_blocker(&running, blocker).await?;
        sqlx::query(
            "UPDATE runner.leases SET cancel_requested_at = clock_timestamp() WHERE id = $1",
        )
        .bind(&running.lease)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    };
    let (verdict, cancelled) = tokio::join!(running.renew_from_old_request(), cancelling);
    cancelled?;
    assert_eq!(verdict, Renewed::Cancelled);
    assert_eq!(snapshot(&running).await, before);
    drop(connection);
    running.cleanup().await;
    Ok(())
}

async fn wait_for_blocker(running: &Running, blocker: i32) -> TestResult {
    use std::time::Duration;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut connection = running.fixtures.database.acquire().await?;
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))")
                .bind(blocker).fetch_one(&mut *connection).await?;
            if waiting { return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(()); }
        }
    }).await?
}
