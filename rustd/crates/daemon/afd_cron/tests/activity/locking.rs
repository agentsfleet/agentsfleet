//! History waits must let the first session's foreign-key check make progress.

use std::time::Duration;

use afd_cron::{Schedule, SyncStatus};
use serde_json::{Value, json};
use sqlx::{Connection as _, PgConnection, Postgres, Transaction};

use super::fixture::*;
use crate::support::CronLane;

const WAIT_BUDGET: Duration = Duration::from_secs(5);
const LOCK_SCHEDULE: &str = "SELECT pg_backend_pid() FROM fleet.schedules WHERE id=$1 FOR UPDATE";
const READ_SESSION: &str = "SELECT context_json FROM fleet.sessions WHERE fleet_id=$1";

// Value: protects=history waits permit a first session's real foreign-key check;
// fails_when=the parent lock conflicts with KEY SHARE and the insert times out;
// seam=native finalization and schema; no_claim=terminal report or runner guard execution.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn schedule_history_wait_allows_a_first_session_to_commit() {
    let lane = CronLane::open().await;
    let token = CronLane::token();
    let row = create(&lane, &token).await;
    let mut connection = lane.connection().await;
    let mut held = connection.begin().await.expect("schedule holder");
    let pid: i32 = sqlx::query_scalar(LOCK_SCHEDULE)
        .bind(&row.schedule_id)
        .fetch_one(&mut *held)
        .await
        .expect("lock this schedule only");
    let (finalized, ()) = tokio::time::timeout(WAIT_BUDGET, async {
        tokio::join!(
            lane.store.finalize_synced(&row, &token, None),
            checkpoint_while_finalizer_waits(&lane, held, pid)
        )
    })
    .await
    .expect("both transactions finish within the scenario budget");
    let finalized = finalized
        .expect("native finalization")
        .expect("current token");
    drop(connection);
    assert_committed(&lane, &row, finalized).await;
}

async fn assert_committed(lane: &CronLane, row: &Schedule, finalized: Schedule) {
    assert_eq!(
        (finalized.sync_status, finalized.sync_token.as_ref()),
        (SyncStatus::Synced, None)
    );
    assert_eq!(stored(lane, &row.schedule_id).await, Some(finalized));
    same_facts(
        receipts(lane).await,
        vec![
            changed(&row.schedule_id, "created", "active", MESSAGE),
            fact(
                &row.schedule_id,
                "schedule_registered",
                "active",
                "synced",
                MESSAGE,
            ),
        ],
    );
    let saved: Value = sqlx::query_scalar(READ_SESSION)
        .bind(&lane.fleet)
        .fetch_one(&mut *lane.connection().await)
        .await
        .expect("the session committed");
    assert_eq!(
        saved,
        json!({"last_event_id":"support-review","last_response":"Support queue reviewed."})
    );
    no_work(lane).await;
}

async fn checkpoint_while_finalizer_waits(
    lane: &CronLane,
    mut held: Transaction<'_, Postgres>,
    pid: i32,
) {
    let mut observer = lane.connection().await;
    wait_for_blocked_finalizer(&mut observer, pid).await;
    let absent: Option<Value> = sqlx::query_scalar(READ_SESSION)
        .bind(&lane.fleet)
        .fetch_optional(&mut *observer)
        .await
        .expect("inspect the first session");
    assert_eq!(
        absent, None,
        "this insert must run the parent foreign-key check"
    );
    sqlx::query("SET LOCAL lock_timeout = '1s'")
        .execute(&mut *held)
        .await
        .expect("bound the lock wait");
    let written = sqlx::query(
        "INSERT INTO fleet.sessions (fleet_id,context_json,created_at,updated_at) \
        VALUES ($1,$2,now(),now())",
    )
    .bind(&lane.fleet)
    .bind(sqlx::types::Json(
        json!({"last_event_id":"support-review","last_response":"Support queue reviewed."}),
    ))
    .execute(&mut *held)
    .await
    .expect("session foreign-key check progresses while finalization waits");
    assert_eq!(written.rows_affected(), 1);
    held.commit()
        .await
        .expect("commit the session and release the schedule");
}

async fn wait_for_blocked_finalizer(observer: &mut PgConnection, pid: i32) {
    tokio::time::timeout(WAIT_BUDGET, async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity \
                WHERE wait_event_type='Lock' AND $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(&mut *observer)
            .await
            .expect("observe the actual row wait");
            if blocked {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("native finalization reaches the locked schedule");
}
