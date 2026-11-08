//! Receipt readers coexist, but still wait for authority writers and recheck fences.
use super::fixture::{Case, exercise};
use af_core::error_code;
use afd_fleet::task::TaskLifecycle as _;
use sqlx::Connection as _;
use std::time::Duration;
use tokio::time::{interval, timeout};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn accepted_replays_share_slot_and_lease_read_locks() {
    exercise(async |case| {
        let plan = case.plan("shared-receipt", 3);
        let first = case
            .tasks
            .bind(case.lease(), &plan)
            .await
            .expect("accepted plan");
        let mut connection = case.held.fixtures.database.acquire().await.expect("reader");
        let mut reader = connection.begin().await.expect("authority read");
        sqlx::query("SELECT fleet_id FROM runner.fleet_slots WHERE fleet_id=$1::uuid FOR SHARE")
            .bind(&case.held.fleet)
            .fetch_one(&mut *reader)
            .await
            .expect("shared slot lock");
        sqlx::query("SELECT id FROM runner.leases WHERE id=$1::uuid FOR SHARE")
            .bind(case.held.lease_key.as_str())
            .fetch_one(&mut *reader)
            .await
            .expect("shared lease lock");
        let replay = case
            .tasks
            .bind(case.lease(), &plan)
            .await
            .expect("receipt readers coexist while both locks remain held");
        assert!(replay.replayed);
        assert_eq!(replay.items, first.items);
        assert_eq!(replay.current_task, first.current_task);
        assert_eq!(replay.execution_deadline, first.execution_deadline);
        reader.commit().await.expect("release reader");
        assert_eq!(case.counts().await, (3, 3, 3));
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn accepted_replays_wait_for_writers_and_recheck_the_fence() {
    exercise(async |case| {
        let plan = case.plan("fenced-receipt", 1);
        let first = case.tasks.bind(case.lease(), &plan).await.expect("accepted plan");
        let mut connection = case.held.fixtures.database.acquire().await.expect("writer");
        let mut writer = connection.begin().await.expect("authority write");
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid() FROM runner.fleet_slots WHERE fleet_id=$1::uuid FOR UPDATE")
            .bind(&case.held.fleet).fetch_one(&mut *writer).await.expect("exclusive slot lock");
        let change = async {
            waiting(case, pid).await;
            sqlx::query("UPDATE runner.fleet_slots SET fencing_token=fencing_token+1 WHERE fleet_id=$1::uuid")
                .bind(&case.held.fleet).execute(&mut *writer).await.expect("new authority fence");
            writer.commit().await.expect("release writer");
        };
        let (replay, ()) = tokio::join!(case.tasks.bind(case.lease(), &plan), change);
        assert_eq!(replay.expect_err("old fence cannot read an execution receipt").code(), error_code::RUN_STALE_FENCING_TOKEN);
        assert_eq!(case.counts().await, (1, 1, 1));
        assert_eq!(first.items.len(), 1);
    }).await;
}

async fn waiting(case: &Case, pid: i32) {
    let observed = async {
        let mut ticks = interval(Duration::from_millis(10));
        loop {
            ticks.tick().await;
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("lock observer"),
            )
            .await
            .expect("native lock wait");
            if blocked {
                break;
            }
        }
    };
    timeout(Duration::from_secs(2), observed)
        .await
        .expect("receipt must wait for the authority writer");
}
