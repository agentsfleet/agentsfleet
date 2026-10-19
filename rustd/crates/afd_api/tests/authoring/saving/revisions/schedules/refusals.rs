//! A refused edit or Apply leaves both setup and schedule intent unchanged.

use super::*;

/// How long the fixture's scheduler sync hold outlives database time: past the
/// whole Apply it must refuse.
const SYNC_HOLD: std::time::Duration = std::time::Duration::from_secs(30);

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn required_tags_and_inherited_input_shape_are_enforced_before_writes() {
    let scheduled = Scheduled::create().await;
    let original = scheduled.read().await;
    assert_eq!(
        scheduled
            .edit(None, json!({"override":null}))
            .await
            .status(),
        StatusCode::PRECONDITION_REQUIRED
    );
    for invalid in [
        json!({"cron":"0 1 * * *"}),
        json!({"override":{}}),
        json!({"override":{"message":"changed"},"status":"paused"}),
        json!({"override":{"cron":"not a cron"}}),
        json!({"override":{"message":null}}),
        json!({"override":{"unknown":true}}),
    ] {
        let response = scheduled.edit(original["etag"].as_str(), invalid).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{}",
            json_body(response).await
        );
    }
    assert_eq!(scheduled.read().await, original);
    scheduled.fleet.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_live_schedule_sync_refuses_the_entire_apply_and_keeps_its_private_draft() {
    let scheduled = Scheduled::create().await;
    let request = scheduled.improvement().await;
    let row = scheduled.read().await;
    sqlx::query(
        "UPDATE fleet.schedules SET sync_token=$2::uuid,sync_lease_expires_at=clock_timestamp()+$3::interval WHERE id=$1::uuid",
    )
        .bind(row["schedule_id"].as_str().expect("schedule ID"))
        .bind(mint().as_str())
        .bind(SYNC_HOLD)
        .execute(
            &mut *scheduled
                .fleet
                .case
                .base
                .db
                .acquire()
                .await
                .expect("sync connection"),
        )
        .await
        .expect("hold sync fence");
    let response = scheduled
        .fleet
        .apply(
            mint().as_str(),
            scheduled.fleet.saved["setup_etag"]
                .as_str()
                .expect("setup tag"),
            &request,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(response).await["current_state"], "syncing");
    assert_eq!(scheduled.fleet.current().await["setup_revision"], "1");
    assert_eq!(scheduled.fleet.revision_count().await, 1);
    assert_eq!(scheduled.read().await["message"], row["message"]);
    scheduled.fleet.case.cleanup().await;
}
