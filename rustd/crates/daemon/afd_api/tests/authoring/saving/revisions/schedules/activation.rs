//! Activation answers active, and inside its limit, whatever its schedules'
//! sync does.
//!
//! PATCH `active` on a live fleet is a setup retry, and like Save and an
//! approved grant it syncs every unsynced trigger schedule under one 5-second
//! limit. A sync that fails or stalls leaves the schedule's own row to say so,
//! never the response.

use super::*;
use sqlx::Connection as _;

/// Past the 5-second limit activation gives its schedules' sync.
const STALL: std::time::Duration = std::time::Duration::from_secs(7);

/// What an activation may take with a stalled sync: the limit, plus the request.
const ACTIVATION_BOUND: std::time::Duration = std::time::Duration::from_secs(6);

/// A sync state the activation step retries.
const FAILED: &str = "failed";

/// Marks the schedule's last sync failed, so activation owes it another try.
async fn unsynced(scheduled: &Scheduled, schedule: &Value) {
    sqlx::query("UPDATE fleet.schedules SET sync_status=$2 WHERE id=$1")
        .bind(crate::authoring::id_field(schedule))
        .bind(FAILED)
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
        .expect("an owed sync");
}

async fn activated(scheduled: &Scheduled) -> Value {
    let response = scheduled
        .fleet
        .patch_resource(
            r#"{"status":"active"}"#,
            scheduled.fleet.saved["etag"].as_str().expect("fleet tag"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_schedule_whose_sync_errors_does_not_fail_the_activation() {
    let scheduled = Scheduled::create().await;
    let row = scheduled.read().await;
    unsynced(&scheduled, &row["schedule_id"]).await;
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

    let activated = activated(&scheduled).await;

    assert_eq!(activated["status"], "active", "{activated}");
    assert_eq!(scheduled.read().await["schedule_id"], row["schedule_id"]);
    scheduled.fleet.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_schedule_whose_sync_stalls_does_not_hold_the_activation_past_its_limit() {
    let scheduled = Scheduled::create().await;
    let row = scheduled.read().await;
    unsynced(&scheduled, &row["schedule_id"]).await;
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

    let activated = tokio::time::timeout(STALL, activated(&scheduled))
        .await
        .expect("the activation answers before the row is released");

    assert!(
        started.elapsed() < ACTIVATION_BOUND,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(activated["status"], "active", "{activated}");
    holder.rollback().await.expect("release the schedule row");
    // Back to the pool before cleanup asks it for one.
    drop(connection);
    scheduled.fleet.case.cleanup().await;
}
