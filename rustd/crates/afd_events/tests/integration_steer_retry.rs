//! A lost response can be retried without accepting a second message.
#![expect(clippy::expect_used, reason = "test assertions")]

use afd_events::{ACTOR_MACHINE, Steer};

use crate::support::EventsLane;

pub(crate) const REQUEST_JSON: &str = r#"{"message":"redeploy staging"}"#;
pub(crate) const KEY_SEPARATOR: &str = ":";
pub(crate) const PRODUCER_STEER: &str = "steer";
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0999";
const DISTINCT_OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0998";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_retry_reuses_its_admission() {
    let lane = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let first = append_with(&steer, &lane, Some(OPERATION)).await;
    let retried = append_with(&steer, &lane, Some(OPERATION)).await;
    assert_eq!(first, retried);
    assert_eq!(admissions_for(&lane, OPERATION).await, 1);
    assert_eq!(queued_ids(&lane).await, [first]);
    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_two_operation_ids_with_equal_text_admit_twice() {
    let lane = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let first = append_with(&steer, &lane, Some(OPERATION)).await;
    let second = append_with(&steer, &lane, Some(DISTINCT_OPERATION)).await;
    assert_ne!(first, second);
    assert_eq!(queued_ids(&lane).await, [first, second]);
    lane.cleanup().await;
}

pub(crate) async fn append_with(
    steer: &Steer,
    lane: &EventsLane,
    operation: Option<&str>,
) -> String {
    steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            operation,
        )
        .await
        .expect("admitted")
        .event_id
}

pub(crate) async fn admissions_for(lane: &EventsLane, operation: &str) -> i64 {
    let key = [lane.fleet.as_str(), operation].join(KEY_SEPARATOR);
    let mut connection = lane.connection().await;
    sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions WHERE producer = $1 AND producer_key = $2",
    )
    .bind(PRODUCER_STEER)
    .bind(key)
    .fetch_one(&mut *connection)
    .await
    .expect("admission count")
}

pub(crate) async fn queued_ids(lane: &EventsLane) -> Vec<String> {
    let mut connection = lane.connection().await;
    sqlx::query_scalar(
        "SELECT id::text FROM fleet.admissions
         WHERE fleet_id = $1::uuid AND settled_at IS NULL ORDER BY created_at, sequence",
    )
    .bind(&lane.fleet)
    .fetch_all(&mut *connection)
    .await
    .expect("queue contents")
}
