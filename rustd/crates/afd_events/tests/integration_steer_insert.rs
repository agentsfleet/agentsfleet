//! A repeated steer is decided from the admission's own insert.
//!
//! The ledger's insert answers a key it already holds with the digest and
//! fleet that row holds, and `Steer::append` compares against those. These
//! tests change the stored row between two sends, so the only way the second
//! send can answer correctly is by reading what the row holds at its insert.
//! That no second statement runs is the append path's own shape
//! (`afd_events::steer`); a count of statements is not something this lane
//! can observe.
//!
//! Marked `#[ignore]` so `make test-unit-all` still compiles and lints this
//! without a datastore; `make test-integration-rustd` runs it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Key, Producer, Reply};
use afd_dragonfly::streams::FleetStreams;
use afd_events::{ACTOR_MACHINE, Steer, Steered};
use afd_wire::event::EventType;

use crate::integration_steer_replay::{CAUSE_PAYLOAD, CHANGED_JSON};
use crate::integration_steer_retry::{KEY_SEPARATOR, PRODUCER_STEER, REQUEST_JSON, clean};
use crate::support::EventsLane;
use afd_core::test_util::trace::Capture;

/// The operation id these tests send twice.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0a21";

/// A digest no payload hashes to.
const FOREIGN_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The webhook delivery a sender redelivers with a body this daemon now renders
/// differently, keyed per fleet the way the webhook producer keys it.
const DELIVERY_ID: &str = "delivery-019feca5-bc9b-72e8";
const DELIVERY_JSON: &str = r#"{"action":"opened"}"#;
const REDELIVERED_JSON: &str = r#"{"action":"opened","draft":false}"#;

/// Rewrites the payload digest, or the fleet, of the row an operation holds.
const SET_DIGEST: &str = "UPDATE core.fleet_admissions SET payload_digest = $3 \
     WHERE producer = $1 AND producer_key = $2";
const SET_FLEET: &str = "UPDATE core.fleet_admissions SET fleet_id = $3::uuid \
     WHERE producer = $1 AND producer_key = $2";

/// The warns a reused key can leave, the field that names them, and the field
/// naming which half of the stored row differed.
const CONFLICT_EVENT: &str = "steer_operation_conflict";
const DRIFT_EVENT: &str = "admission_payload_drifted";
const FIELD_EVENT: &str = "event";
const FIELD_CAUSE: &str = "cause";
const CAUSE_FLEET: &str = "fleet";

/// A repeat is answered from the insert: the first event, marked a replay.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_replayed_append_decides_from_its_insert() {
    let lane = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());

    let first = send(&steer, &lane, REQUEST_JSON)
        .await
        .expect("the first send is admitted");
    let again = send(&steer, &lane, REQUEST_JSON)
        .await
        .expect("the repeat is answered");
    assert!(!first.replayed, "a fresh admission is not a replay");
    assert_eq!(
        again,
        Steered {
            event_id: first.event_id,
            replayed: true
        }
    );

    clean(&lane, &FleetStreams::new(lane.queue.clone())).await;
}

/// A repeat whose stored row holds another payload, or another fleet, is
/// refused — never answered with that row's event.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_replayed_append_never_answers_unchecked() {
    let lane = EventsLane::open().await;
    let elsewhere = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());
    send(&steer, &lane, REQUEST_JSON)
        .await
        .expect("the first send is admitted");

    set_stored(&lane, SET_DIGEST, FOREIGN_DIGEST).await;
    let logs = Capture::install();
    let foreign_digest = send(&steer, &lane, REQUEST_JSON)
        .await
        .expect_err("a row holding another payload is refused");
    assert!(foreign_digest.is_operation_conflict(), "{foreign_digest}");
    assert_eq!(causes(&logs), [CAUSE_PAYLOAD]);
    drop(logs);

    set_stored(&lane, SET_FLEET, &elsewhere.fleet).await;
    let logs = Capture::install();
    let foreign_fleet = send(&steer, &lane, REQUEST_JSON)
        .await
        .expect_err("a row holding another fleet is refused");
    assert!(foreign_fleet.is_operation_conflict(), "{foreign_fleet}");
    assert_eq!(causes(&logs), [CAUSE_FLEET], "the fleet is named first");
    drop(logs);

    clean(&lane, &FleetStreams::new(lane.queue.clone())).await;
}

/// A steer's reused id is the caller's conflict, logged once as one; a
/// webhook's redelivery that drifted is still the deploy warn it always was.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_reuse_is_not_an_internal_failure() {
    let lane = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());
    send(&steer, &lane, REQUEST_JSON)
        .await
        .expect("the first send is admitted");

    let logs = Capture::install();
    send(&steer, &lane, CHANGED_JSON)
        .await
        .expect_err("a reused id with another message is refused");
    assert_eq!(count(&logs, CONFLICT_EVENT), 1);
    assert_eq!(
        count(&logs, DRIFT_EVENT),
        0,
        "a caller's conflict is not drift"
    );
    drop(logs);

    let admissions = lane.admissions();
    let key = [lane.fleet.as_str(), DELIVERY_ID].join(KEY_SEPARATOR);
    admissions
        .admit(delivery(&lane, &key, DELIVERY_JSON))
        .await
        .expect("the delivery is admitted");
    let logs = Capture::install();
    let redelivered = admissions
        .admit(delivery(&lane, &key, REDELIVERED_JSON))
        .await
        .expect("a redelivery is answered, never refused");
    assert!(redelivered.replayed);
    assert_eq!(
        count(&logs, DRIFT_EVENT),
        1,
        "a drifted redelivery still warns"
    );
    drop(logs);

    clean(&lane, &FleetStreams::new(lane.queue.clone())).await;
}

/// One steer of `request_json` under [`OPERATION`].
async fn send(steer: &Steer, lane: &EventsLane, request_json: &str) -> afd_events::Result<Steered> {
    steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            request_json,
            Some(OPERATION),
        )
        .await
}

/// Runs `statement` over the row [`OPERATION`] holds on the lane's fleet.
async fn set_stored(lane: &EventsLane, statement: &'static str, value: &str) {
    let key = [lane.fleet.as_str(), OPERATION].join(KEY_SEPARATOR);
    let mut connection = lane.connection().await;
    let updated = sqlx::query(statement)
        .bind(PRODUCER_STEER)
        .bind(key)
        .bind(value)
        .execute(&mut *connection)
        .await
        .expect("the stored row is rewritten");
    assert_eq!(updated.rows_affected(), 1, "one row holds the operation");
}

/// A webhook delivery on the lane's fleet under `key`.
fn delivery<'a>(lane: &'a EventsLane, key: &'a str, request_json: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(key),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: ACTOR_MACHINE,
        event_type: EventType::Webhook,
        request_json,
        reply: Reply::None,
    }
}

/// The `cause` each recorded conflict warn names, in order.
fn causes(logs: &Capture) -> Vec<String> {
    logs.events()
        .into_iter()
        .filter(|record| record.fields.get(FIELD_EVENT).map(String::as_str) == Some(CONFLICT_EVENT))
        .filter_map(|record| record.fields.get(FIELD_CAUSE).cloned())
        .collect()
}

/// How many recorded events carry `event`.
fn count(logs: &Capture, event: &str) -> usize {
    logs.events()
        .iter()
        .filter(|record| record.fields.get(FIELD_EVENT).map(String::as_str) == Some(event))
        .count()
}
