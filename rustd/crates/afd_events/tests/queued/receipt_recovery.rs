//! A durable admission remains readable when its append receipt was lost.

#![expect(
    clippy::indexing_slicing,
    reason = "a check indexes the page it has just asserted holds one row"
)]

use super::*;

const CLEAR_RECEIPT: &str = "\
UPDATE fleet.admissions SET receipt = NULL
WHERE fleet_id = $1::uuid AND delivered_at IS NULL";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_receiptless_admission_is_visible_and_replay_keeps_its_identity() {
    let lane = EventsLane::open().await;
    let operation = afd_db::test_util::mint_id();
    let steer = Steer::new(lane.admissions());
    let accepted = steer
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, Some(&operation))
        .await
        .expect("durable admission succeeds");
    lose_receipt(&lane).await;
    assert_queued(&lane, &accepted.event_id).await;
    let replay = steer
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, Some(&operation))
        .await
        .expect("retry repairs the same admission");
    assert_eq!(replay.event_id, accepted.event_id);
    assert_queued(&lane, &accepted.event_id).await;
    mark_delivered(&lane).await;
    lane.seed_event(
        &accepted.event_id,
        accepted.admitted_at.expect("first admission time"),
    )
    .await;
    assert_delivered(&lane, &accepted.event_id).await;
    clean(&lane, &FleetStreams::new(lane.queue.clone())).await;
    lane.cleanup().await;
}

async fn lose_receipt(lane: &EventsLane) {
    let mut connection = lane.connection().await;
    sqlx::query(CLEAR_RECEIPT)
        .bind(&lane.fleet)
        .execute(&mut *connection)
        .await
        .expect("simulate the receipt write being lost");
}

async fn assert_queued(lane: &EventsLane, event_id: &str) {
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (uuid(&lane.workspace), uuid(&lane.fleet));
    let pending = history
        .thread_page(&workspace, &fleet, None, PAGE)
        .await
        .expect("admissions can be read before receipt recovery");
    assert_eq!(pending.len(), 1, "an accepted message must survive reload");
    assert_eq!(pending[0].row.event_id, event_id);
    assert_eq!(pending[0].row.status, status::QUEUED);
    assert_eq!(pending[0].request_json, BODY);
}

async fn mark_delivered(lane: &EventsLane) {
    let mut connection = lane.connection().await;
    sqlx::query(MARK_DELIVERED)
        .bind(&lane.fleet)
        .execute(&mut *connection)
        .await
        .expect("delivery is recorded");
}

async fn assert_delivered(lane: &EventsLane, event_id: &str) {
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (uuid(&lane.workspace), uuid(&lane.fleet));
    let delivered = history
        .thread_page(&workspace, &fleet, None, PAGE)
        .await
        .expect("delivered history keeps the logical identity");
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].row.event_id, event_id);
    assert_ne!(delivered[0].row.status, status::QUEUED);
}
