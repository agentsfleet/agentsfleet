//! Steers preserve their identity and envelope in the serving queue.
#![expect(clippy::expect_used, reason = "test assertions")]

use afd_events::{ACTOR_MACHINE, Steer};
use afd_wire::event::EventType;

use crate::integration_steer_retry::{REQUEST_JSON, append_with, queued_ids};
use crate::support::EventsLane;

#[derive(Debug, sqlx::FromRow)]
struct Envelope {
    id: String,
    actor: String,
    event_type: String,
    workspace: String,
    request_json: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: jiff::Timestamp,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_returns_the_queued_identity_and_preserves_its_envelope() {
    let lane = EventsLane::open().await;
    let accepted = Steer::new(lane.admissions())
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            None,
        )
        .await
        .expect("admitted");
    let mut connection = lane.connection().await;
    let row = sqlx::query_as::<_, Envelope>(
        "SELECT id::text, actor, event_type, workspace_id::text AS workspace,
                request_json, event_created_at AS created_at
           FROM fleet.admissions WHERE fleet_id = $1::uuid AND settled_at IS NULL",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("one serving queue row");
    assert_eq!(row.id, accepted.event_id);
    assert_eq!(row.actor, ACTOR_MACHINE);
    assert_eq!(row.event_type, EventType::Chat.as_str());
    assert_eq!(row.workspace, lane.workspace);
    assert_eq!(row.request_json, REQUEST_JSON);
    assert_eq!(Some(row.created_at), accepted.admitted_at);
    drop(connection);
    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_two_keyless_sends_of_equal_text_are_two_queued_messages() {
    let lane = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let first = append_with(&steer, &lane, None).await;
    let second = append_with(&steer, &lane, None).await;
    assert_ne!(first, second);
    assert_eq!(queued_ids(&lane).await, [first, second]);
    lane.cleanup().await;
}
