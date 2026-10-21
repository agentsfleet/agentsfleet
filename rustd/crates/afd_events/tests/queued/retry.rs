//! Repeated sends preserve one waiting message and its delivered identity.

#![expect(
    clippy::indexing_slicing,
    reason = "a check indexes the page it has just asserted holds one row"
)]

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_retry_keeps_one_queued_message_and_its_delivered_identity() {
    let lane = EventsLane::open().await;
    let operation = afd_db::test_util::mint_id();
    let steer = Steer::new(lane.admissions());
    let accepted = steer
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, Some(&operation))
        .await
        .expect("durable admission succeeds");
    assert_queued(&lane, &accepted.event_id).await;
    let replay = steer
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, Some(&operation))
        .await
        .expect("retry returns the same admission");
    assert_eq!(replay.event_id, accepted.event_id);
    assert_queued(&lane, &accepted.event_id).await;
    mark_delivered(&lane).await;
    lane.seed_event(
        &accepted.event_id,
        accepted.admitted_at.expect("first admission time"),
    )
    .await;
    assert_delivered(&lane, &accepted.event_id).await;
    lane.cleanup().await;
}

async fn assert_queued(lane: &EventsLane, event_id: &str) {
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (uuid(&lane.workspace), uuid(&lane.fleet));
    let pending = history
        .thread_page(&workspace, &fleet, None, PAGE)
        .await
        .expect("admissions can be read after acceptance");
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
