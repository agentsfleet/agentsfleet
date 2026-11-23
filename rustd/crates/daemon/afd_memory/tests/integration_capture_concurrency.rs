//! Captures serialize one fleet without occupying unrelated fleets.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use crate::workspace::{Grants, Workspace, delta};
use af_wire::memory::Visibility;
use sqlx::Connection as _;
use std::time::Duration;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_captures_serialize_one_fleet_and_leave_other_fleets_available() {
    let space = Workspace::create().await;
    let fleet = space.fleet("Code review", Grants::default()).await;
    let other = space.fleet("Support triage", Grants::default()).await;
    let mut connection = space.database.acquire().await.expect("blocker");
    let mut tx = connection.begin().await.expect("transaction");
    sqlx::query("SELECT id FROM fleet.fleets WHERE id=$1 FOR NO KEY UPDATE")
        .bind(&fleet)
        .execute(&mut *tx)
        .await
        .expect("hold parent");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx)
        .await
        .expect("blocker identity");
    let left = capture(&space, &fleet, "first");
    let right = capture(&space, &fleet, "second");
    wait_for_two(&space, pid).await;
    tokio::time::timeout(
        Duration::from_secs(5),
        space.memories.capture(
            &other,
            &[delta("independent", "core", Visibility::Fleet)],
            af_core::clock::now(),
        ),
    )
    .await
    .expect("other fleet proceeds")
    .expect("capture");
    tx.commit().await.expect("release contenders");
    left.await.expect("first task").expect("first capture");
    right.await.expect("second task").expect("second capture");
    let mut connection = space.database.acquire().await.expect("count");
    let mut tx = connection.begin().await.expect("role transaction");
    sqlx::query("SET LOCAL ROLE memory_runtime")
        .execute(&mut *tx)
        .await
        .expect("memory role");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.entries WHERE fleet_id=$1")
        .bind(&fleet)
        .fetch_one(&mut *tx)
        .await
        .expect("entry count");
    assert_eq!(count, 2, "both serialized captures land");
    tx.commit().await.expect("end role");
    drop(connection);
    space.cleanup().await;
}

fn capture(
    space: &Workspace,
    fleet: &af_core::id::Uuid7,
    key: &'static str,
) -> tokio::task::JoinHandle<afd_memory::Result<afd_memory::Captured>> {
    let (memories, fleet) = (space.memories.clone(), fleet.clone());
    tokio::spawn(async move {
        memories
            .capture(
                &fleet,
                &[delta(key, "core", Visibility::Fleet)],
                af_core::clock::now(),
            )
            .await
    })
}

async fn wait_for_two(space: &Workspace, blocker: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "WITH RECURSIVE waiters(pid) AS (SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) UNION SELECT activity.pid FROM pg_stat_activity activity JOIN waiters ON waiters.pid=ANY(pg_blocking_pids(activity.pid))) SELECT count(*) FROM waiters",
            )
            .bind(blocker)
            .fetch_one(&mut *space.database.acquire().await.expect("observer"))
            .await
            .expect("waiting captures");
            if waiting >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both captures wait on the fleet before writing");
}
