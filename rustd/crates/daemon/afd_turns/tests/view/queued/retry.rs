//! Repeated sends preserve one waiting message and its delivered identity.

#![expect(
    clippy::indexing_slicing,
    reason = "a check indexes the page it has just asserted holds one row"
)]

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_retry_keeps_one_queued_message_and_its_delivered_identity() {
    let lane = TurnsLane::open().await;
    let operation = afd_db::test_util::mint_id();
    let steer = Steer::new(lane.turns());
    let accepted = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR,
            BODY,
            Some(operation.as_str()),
        )
        .await
        .expect("durable turn succeeds");
    assert_queued(&lane, &accepted.turn_id).await;
    let replay = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR,
            BODY,
            Some(operation.as_str()),
        )
        .await
        .expect("retry returns the same turn");
    assert_eq!(replay.turn_id, accepted.turn_id);
    assert_queued(&lane, &accepted.turn_id).await;
    mark_delivered(&lane).await;
    lane.seed_event(
        &uuid(&accepted.turn_id),
        accepted.admitted_at.expect("first turn time"),
    )
    .await;
    assert_delivered(&lane, &accepted.turn_id).await;
    lane.cleanup().await;
}

async fn assert_queued(lane: &TurnsLane, turn_id: &str) {
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (&lane.workspace, &lane.fleet);
    let pending = history
        .thread_page(workspace, fleet, None, PAGE)
        .await
        .expect("turns can be read after acceptance");
    assert_eq!(pending.len(), 1, "an accepted message must survive reload");
    assert_eq!(pending[0].row.turn_id, turn_id);
    assert_eq!(pending[0].row.status, status::QUEUED);
    assert_eq!(pending[0].request_json, BODY);
}

async fn mark_delivered(lane: &TurnsLane) {
    let mut connection = lane.connection().await;
    sqlx::query(MARK_DELIVERED)
        .bind(&lane.fleet)
        .execute(&mut *connection)
        .await
        .expect("delivery is recorded");
}

async fn assert_delivered(lane: &TurnsLane, turn_id: &str) {
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (&lane.workspace, &lane.fleet);
    let delivered = history
        .thread_page(workspace, fleet, None, PAGE)
        .await
        .expect("delivered history keeps the logical identity");
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].row.turn_id, turn_id);
    assert_ne!(delivered[0].row.status, status::QUEUED);
}
