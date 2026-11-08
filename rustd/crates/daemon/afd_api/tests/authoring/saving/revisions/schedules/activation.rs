//! Resuming a fleet answers active, and inside its limit, whatever its schedules' sync does.
//!
//! Activation syncs every unsynced trigger schedule under one 5-second limit and
//! answers with the setup regardless: a sync that fails or stalls leaves the
//! schedule's own row to say so, never the resume.

use super::*;
use sqlx::Connection as _;

/// Past the 5-second limit activation gives its schedules' sync.
const STALL: std::time::Duration = std::time::Duration::from_secs(7);

/// What a resume may take with a stalled sync: the limit, plus the request.
const RESUME_BOUND: std::time::Duration = std::time::Duration::from_secs(6);

async fn stopped(scheduled: &Scheduled) -> Value {
    let response = scheduled
        .fleet
        .patch_resource(
            r#"{"status":"stopped"}"#,
            scheduled.fleet.saved["etag"].as_str().expect("fleet tag"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn resumed(scheduled: &Scheduled, stopped: &Value) -> Value {
    let response = scheduled
        .fleet
        .patch_resource(
            r#"{"status":"active"}"#,
            stopped["etag"].as_str().expect("stopped tag"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_schedule_whose_sync_errors_does_not_fail_the_resume() {
    let scheduled = Scheduled::create().await;
    let stopped = stopped(&scheduled).await;
    let row = scheduled.read().await;
    // The claim bumps `resource_version`, so a row already at the ceiling makes
    // the database refuse the claim itself: a sync error, not a refused push.
    sqlx::query("UPDATE fleet.schedules SET resource_version=9223372036854775807 WHERE id=$1")
        .bind(crate::authoring::id_field(&row["schedule_id"]))
        .execute(
            &mut *scheduled
                .fleet
                .case
                .base
                .db
                .acquire()
                .await
                .expect("schedule connection"),
        )
        .await
        .expect("seed the version ceiling");

    let resumed = resumed(&scheduled, &stopped).await;

    assert_eq!(resumed["status"], "active", "{resumed}");
    assert_eq!(scheduled.read().await["schedule_id"], row["schedule_id"]);
    scheduled.fleet.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_schedule_whose_sync_stalls_does_not_hold_the_resume_past_its_limit() {
    let scheduled = Scheduled::create().await;
    let stopped = stopped(&scheduled).await;
    let row = scheduled.read().await;
    // Another session holds the row, so the sync's claim waits on it.
    let mut connection = scheduled
        .fleet
        .case
        .base
        .db
        .acquire()
        .await
        .expect("lock connection");
    let mut holder = connection.begin().await.expect("lock transaction");
    sqlx::query("SELECT id FROM fleet.schedules WHERE id=$1 FOR UPDATE")
        .bind(crate::authoring::id_field(&row["schedule_id"]))
        .execute(&mut *holder)
        .await
        .expect("hold the schedule row");
    let started = std::time::Instant::now();

    let resumed = tokio::time::timeout(STALL, resumed(&scheduled, &stopped))
        .await
        .expect("the resume answers before the row is released");

    assert!(started.elapsed() < RESUME_BOUND, "{:?}", started.elapsed());
    assert_eq!(resumed["status"], "active", "{resumed}");
    holder.rollback().await.expect("release the schedule row");
    // Back to the pool before cleanup asks it for one.
    drop(connection);
    scheduled.fleet.case.cleanup().await;
}
