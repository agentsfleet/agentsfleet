//! A delayed capture rechecks its fence and grants after the fleet lock.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use crate::report_seed::{Held, held};
use af_core::error_code;
use af_wire::memory::{MemoryDelta, MemoryPushRequest, Visibility};
use sqlx::Connection as _;
use std::{borrow::Cow, time::Duration};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn delayed_capture_cannot_overwrite_after_a_higher_fence_commits() {
    capture_after_change(true).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn delayed_capture_observes_a_publication_revocation() {
    capture_after_change(false).await;
}

async fn capture_after_change(supersede: bool) {
    let run = held().await;
    let memories = afd_memory::Memories::new(
        run.fixtures.database.clone(),
        af_crypto::entropy::Entropy::new(),
    );
    sqlx::query("UPDATE fleet.fleets SET memory_publishes_workspace=true WHERE id=$1")
        .bind(&run.fleet)
        .execute(&mut *run.fixtures.database.acquire().await.expect("grant"))
        .await
        .expect("allow initial publication");
    let mut connection = run.fixtures.database.acquire().await.expect("blocker");
    let mut tx = connection.begin().await.expect("transaction");
    sqlx::query("SELECT id FROM fleet.fleets WHERE id=$1 FOR NO KEY UPDATE")
        .bind(&run.fleet)
        .execute(&mut *tx)
        .await
        .expect("hold grant row");
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx)
        .await
        .expect("blocker identity");
    let capture = delayed_capture(&run);
    wait_for_capture(&run.fixtures.database, blocker).await;
    change_authority(&run, &memories, &mut tx, supersede).await;
    tx.commit().await.expect("authority change commits");
    let result = capture.await.expect("capture task");
    assert_capture(&run, &memories, result, supersede).await;
    run.fixtures.cleanup().await;
}

fn delayed_capture(run: &Held) -> tokio::task::JoinHandle<afd_fleet::Result<afd_memory::Captured>> {
    tokio::spawn({
        let (plane, runner, fleet) = (run.fixtures.plane(), run.runner.clone(), run.fleet.clone());
        let request = MemoryPushRequest {
            lease_id: Cow::Owned(run.issued.lease_id.to_string()),
            fencing_token: run.fence.as_u64(),
            memory: vec![entry("stale", Visibility::Workspace)],
        };
        async move {
            plane
                .capture(&runner, &fleet, &request, af_core::clock::now())
                .await
        }
    })
}

async fn change_authority(
    run: &Held,
    memories: &afd_memory::Memories,
    connection: &mut sqlx::PgConnection,
    supersede: bool,
) {
    if supersede {
        sqlx::query("UPDATE runner.slots SET fencing_token=fencing_token+1 WHERE fleet_id=$1")
            .bind(&run.fleet)
            .execute(&mut *connection)
            .await
            .expect("replacement fence");
        memories
            .capture_on(
                connection,
                &run.fleet,
                &[entry("new holder", Visibility::Fleet)],
                run.now,
            )
            .await
            .expect("replacement writes");
    } else {
        sqlx::query("UPDATE fleet.fleets SET memory_publishes_workspace=false WHERE id=$1")
            .bind(&run.fleet)
            .execute(&mut *connection)
            .await
            .expect("revoke publication");
    }
}

async fn assert_capture(
    run: &Held,
    memories: &afd_memory::Memories,
    result: afd_fleet::Result<afd_memory::Captured>,
    supersede: bool,
) {
    if supersede {
        assert_eq!(
            result.expect_err("old fence refused").code(),
            error_code::RUN_STALE_FENCING_TOKEN
        );
        let stored = memories.hydrate(&run.fleet).await.expect("memory");
        assert_eq!(stored.memory.len(), 1);
        assert_eq!(
            stored.memory.first().expect("one entry").content,
            "new holder"
        );
    } else {
        let stored = result.expect("private capture continues");
        assert_eq!((stored.stored, stored.unpublished), (0, 1));
        assert!(
            memories
                .hydrate(&run.fleet)
                .await
                .expect("memory")
                .memory
                .is_empty(),
            "revoked publication must not store workspace memory"
        );
    }
}

fn entry(content: &'static str, visibility: Visibility) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Borrowed("decision"),
        content: Cow::Borrowed(content),
        category: Cow::Borrowed("core"),
        visibility,
    }
}

async fn wait_for_capture(database: &afd_db::Db, blocker: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker)
            .fetch_one(&mut *database.acquire().await.expect("observer"))
            .await
            .expect("capture wait");
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("capture queues behind the grant row");
}
