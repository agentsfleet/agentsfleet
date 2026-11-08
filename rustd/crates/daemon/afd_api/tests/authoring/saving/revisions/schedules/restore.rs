//! Apply preserves the instance overlay; Restore reads the newly pinned recipe.

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn overlay_survives_apply_after_registration_and_restore_uses_the_new_base() {
    let scheduled = Scheduled::create().await;
    let overlaid = pause_with_instance_intent(&scheduled).await;
    let id = overlaid["schedule_id"].as_str().expect("schedule ID");
    record_registered_identity(&scheduled, id).await;
    apply_revised_recipe(&scheduled).await;
    let current = scheduled.read().await;
    assert_eq!(current["schedule_id"], overlaid["schedule_id"]);
    assert_eq!(current["message"], "Instance intent");
    assert_eq!(current["cron"], "0 17 * * *");
    assert_eq!(current["status"], "paused");
    assert_eq!(current["override"], overlaid["override"]);
    let stale = scheduled
        .edit(overlaid["etag"].as_str(), json!({"override":null}))
        .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(json_body(stale).await["etag"], current["etag"]);
    let restored = scheduled
        .edit(current["etag"].as_str(), json!({"override":null}))
        .await;
    assert_eq!(restored.status(), StatusCode::OK);
    let restored = json_body(restored).await;
    assert_eq!(restored["message"], "Revised base");
    assert_eq!(restored["cron"], "0 17 * * *");
    assert_eq!(restored["status"], "active");
    assert!(restored.get("override").is_none());
    scheduled.fleet.case.cleanup().await;
}

/// Overlays an instance message and a paused status on the inherited
/// schedule, and answers the overlaid row.
async fn pause_with_instance_intent(scheduled: &Scheduled) -> Value {
    let first = scheduled.read().await;
    let response = scheduled
        .edit(
            first["etag"].as_str(),
            json!({"override":{"message":"Instance intent","status":"paused"}}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let overlaid = json_body(response).await;
    assert_eq!(overlaid["message"], "Instance intent");
    assert_eq!(overlaid["status"], "paused");
    overlaid
}

/// Stands in for a successful upstream registration, which replaces
/// `source_key`. Inheritance must keep its own identity so a subsequent Apply
/// does not allocate a new row.
async fn record_registered_identity(scheduled: &Scheduled, id: &str) {
    sqlx::query("UPDATE fleet.schedules SET provider_schedule_id=$2 WHERE id=$1::uuid")
        .bind(id)
        .bind(format!("fleet-schedule-{id}"))
        .execute(
            &mut *scheduled
                .fleet
                .case
                .base
                .db
                .acquire()
                .await
                .expect("key connection"),
        )
        .await
        .expect("record registered identity");
}

/// Applies an improvement whose recipe moves the schedule to the revised base.
async fn apply_revised_recipe(scheduled: &Scheduled) {
    let request = scheduled.improvement().await;
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
    let status = response.status();
    let applied = json_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{applied}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn restore_after_repeated_applies_reads_the_latest_base_and_admits_one_of_two_racers() {
    let scheduled = Scheduled::create().await;
    let overlaid = pause_with_instance_intent(&scheduled).await;
    let id = overlaid["schedule_id"].as_str().expect("schedule ID");
    record_registered_identity(&scheduled, id).await;
    apply_revised_recipe(&scheduled).await;
    apply_second_recipe(&scheduled).await;
    let current = scheduled.read().await;
    assert_eq!(current["schedule_id"], overlaid["schedule_id"]);
    assert_eq!(
        current["message"], "Instance intent",
        "the overlay survives two Applies"
    );
    assert_eq!(current["status"], "paused");
    let mut answers = restore_twice(&scheduled, current["etag"].as_str()).await;
    answers.sort_by_key(|(status, _)| status.as_u16());
    let ((won, restored), (lost, refused)) = (&answers[0], &answers[1]);
    assert_eq!(
        (*won, *lost),
        (StatusCode::OK, StatusCode::PRECONDITION_FAILED),
        "{answers:?}"
    );
    assert_eq!(
        restored["message"], "Second revised base",
        "Restore reads the latest base"
    );
    assert_eq!(restored["cron"], "0 18 * * *");
    assert_eq!(restored["status"], "active");
    assert!(restored.get("override").is_none());
    assert_eq!(
        refused["etag"], restored["etag"],
        "the loser learns the winner's tag"
    );
    assert_eq!(scheduled.read().await["etag"], restored["etag"]);
    scheduled.fleet.case.cleanup().await;
}

/// Applies a second improvement, based on revision 2, that moves the schedule
/// again, under the fleet's current setup tag.
async fn apply_second_recipe(scheduled: &Scheduled) {
    let second = scheduled
        .recipe_on(&json!("2"), "0 18 * * *", "Second revised base")
        .await;
    let base = scheduled.fleet.current().await;
    let tag = base["setup_etag"].as_str().expect("revised tag");
    let response = scheduled.fleet.apply(mint().as_str(), tag, &second).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

/// Sends two Restores under one schedule tag at once, bounded, and answers
/// each status with its body.
async fn restore_twice(scheduled: &Scheduled, tag: Option<&str>) -> Vec<(StatusCode, Value)> {
    let restore = json!({"override":null});
    let (first, rival) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        tokio::join!(
            scheduled.edit(tag, restore.clone()),
            scheduled.edit(tag, restore.clone())
        )
    })
    .await
    .expect("rival Restores finish without a deadlock");
    let mut answers = Vec::new();
    for response in [first, rival] {
        let status = response.status();
        answers.push((status, json_body(response).await));
    }
    answers
}
