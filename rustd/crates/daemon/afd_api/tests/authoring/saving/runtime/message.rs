//! A message's operation id across an archive and across a workspace boundary, on
//! a fleet created through Save.

use af_core::error_code;
use af_core::id::Uuid7;
use afd_bus::{Received, Subject};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::super::super::{json_body, mint, send};
use super::Saved;

/// The operation id a client repeats across its retries.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0b01";

/// A second operation id, for new work the archived fleet must still refuse.
const FRESH_OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0b02";

/// The message the operation first carried.
const MESSAGE: &str = "roll back staging";

fn code(body: &Value) -> Option<&str> {
    body.get("error_code").and_then(Value::as_str)
}

/// How long a committed send's wake may take to reach a reader.
const WAKE_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// The marker a test publishes after the send, so a second wake would arrive
/// before it.
const NO_MORE_WAKES: &str = "no-more-wakes";

/// The next message on `wakes`, within [`WAKE_BUDGET`].
async fn next_wake(wakes: &mut afd_bus::Subscription) -> String {
    let received = tokio::time::timeout(WAKE_BUDGET, wakes.recv())
        .await
        .expect("a wake within its budget")
        .expect("the wake subscription stays open");
    let Received::Message(message) = received else {
        panic!("a published wake, not a gap: {received:?}");
    };
    message.payload.as_ref().clone()
}

/// A person's send writes one queued turn on the fleet's thread, and its
/// thread's wake is published once, after the turn is committed.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_fleet_message_writes_a_queued_turn() {
    let saved = Saved::create().await;
    let subject = Subject::ThreadWake(saved.fleet.clone());
    let mut wakes = saved
        .case
        .bus
        .subscribe(&subject)
        .await
        .expect("the lane's NATS subscribes");

    let (status, admitted) = saved.steer(&saved.thread, mint().as_str(), MESSAGE).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{admitted}");
    let turn = Uuid7::parse(admitted["turn_id"].as_str().expect("a 202 names its turn"))
        .expect("a canonical turn id");

    assert_eq!(next_wake(&mut wakes).await, saved.fleet.as_str());
    // Read once the wake has arrived: the turn it rang for is already there.
    let queued: Vec<(Uuid7, String, String)> = sqlx::query_as(
        "SELECT id, status, request_json->>'message' FROM thread.turns \
         WHERE thread_id = $1 AND producer IS NOT NULL",
    )
    .bind(&saved.fleet)
    .fetch_all(&mut *saved.case.base.db.acquire().await.expect("a connection"))
    .await
    .expect("the thread's turns");
    assert_eq!(
        queued,
        vec![(
            turn,
            af_core::turn::status::QUEUED.to_owned(),
            MESSAGE.to_owned()
        )]
    );
    saved
        .case
        .bus
        .publish(&subject, NO_MORE_WAKES)
        .await
        .expect("the marker publishes");
    assert_eq!(
        next_wake(&mut wakes).await,
        NO_MORE_WAKES,
        "one wake for one send"
    );
    drop(wakes);
    saved.case.cleanup().await;
}

/// A fleet saved with no trigger is one only ever talked to: Save stores its
/// set as empty, and a steer is admitted all the same, because nothing on the
/// message path reads what a fleet declares may wake it.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_fleet_with_no_trigger_accepts_a_steer() {
    let saved = Saved::create().await;

    let (status, fleet) = saved.get(&saved.item).await;
    assert_eq!(status, StatusCode::OK, "{fleet}");
    assert_eq!(
        fleet["definition"]["config"]["x-agentsfleet"]["triggers"],
        json!([]),
        "the saved fixture declares no trigger: {fleet}"
    );

    let (status, admitted) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{admitted}");
    assert!(
        admitted["turn_id"]
            .as_str()
            .is_some_and(|event| !event.is_empty()),
        "a steer to a trigger-less fleet is admitted as an event: {admitted}"
    );

    saved.case.cleanup().await;
}

/// The 202 says whether it answered an earlier send: `false` for the send that
/// admitted the message, `true` for a repeat while the fleet runs and after it
/// is archived.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_steer_202_names_a_replay() {
    let saved = Saved::create().await;
    let replayed = |body: &Value| body.get("replayed").and_then(Value::as_bool);

    let (_, fresh) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(replayed(&fresh), Some(false), "{fresh}");
    let (_, repeat) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(replayed(&repeat), Some(true), "{repeat}");
    saved.archive().await;
    let (_, archived) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(replayed(&archived), Some(true), "{archived}");
    assert_eq!(fresh.get("turn_id"), archived.get("turn_id"));

    saved.case.cleanup().await;
}

/// A retry of an admitted message is answered with its event even after the
/// fleet is archived; new work and a changed message are refused.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_replay_bypasses_ingress_refusal() {
    let saved = Saved::create().await;

    let (status, first) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");
    let event = first["turn_id"]
        .as_str()
        .expect("a 202 names its event")
        .to_owned();
    saved.archive().await;

    let (status, retried) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "answered, archived or not: {retried}"
    );
    assert_eq!(retried["turn_id"].as_str(), Some(event.as_str()));

    let (status, changed) = saved
        .steer(&saved.thread, OPERATION, "roll back production")
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{changed}");
    assert_eq!(
        code(&changed),
        Some(error_code::AGENTSFLEET_OPERATION_CONFLICT.as_str())
    );
    assert_eq!(changed["current_state"], "admitted");

    let (status, fresh) = saved.steer(&saved.thread, FRESH_OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::CONFLICT, "{fresh}");
    assert_eq!(
        code(&fresh),
        Some(error_code::AGENTSFLEET_NOT_ACTIVE.as_str())
    );

    assert_keyless_message_refused(&saved).await;

    saved.case.cleanup().await;
}

/// Without an id there is no earlier turn to answer: new work, refused.
async fn assert_keyless_message_refused(saved: &Saved) {
    let keyless = json!({"message": MESSAGE}).to_string();
    let unnamed = send(
        &saved.router,
        Method::POST,
        &saved.thread,
        Some(saved.token()),
        &keyless,
    )
    .await;
    assert_eq!(unnamed.status(), StatusCode::CONFLICT);
    assert_eq!(
        code(&json_body(unnamed).await),
        Some(error_code::AGENTSFLEET_NOT_ACTIVE.as_str())
    );
}

/// A guessed operation id on a fleet this workspace does not hold is a 404,
/// the same answer an unknown fleet gets — never a 409 that would tell the
/// caller the id exists in another tenant's ledger.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_foreign_fleet_replay_is_not_found() {
    let owner = Saved::create().await;
    let (status, first) = owner.steer(&owner.thread, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");

    // Another tenant, naming the owner's fleet under its own workspace, with
    // the same id and the same message.
    let prober = Saved::create().await;
    let probe = format!(
        "{}/fleets/{}/messages",
        prober.workspace,
        owner.fleet.as_str()
    );
    let (status, answer) = prober.steer(&probe, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");
    assert_eq!(
        code(&answer),
        Some(error_code::AGENTSFLEET_NOT_FOUND.as_str())
    );

    prober.case.cleanup().await;
    owner.case.cleanup().await;
}

/// A tenant key the status-blind router authenticates.
const STATUS_BLIND_KEY: &str =
    "agt_tfeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface";

/// No status read precedes the admission: with the fleets store unreachable
/// and the steering ledger live over an active fleet, a steer answers 202. The
/// route once read the status first and answered 503 here.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_steer_reads_no_fleet_status_before_admitting() {
    let saved = Saved::create().await;
    let workspace = saved.case.base.workspace.clone();
    let router = crate::harness::Fleet::new()
        .with_owned_workspace(workspace)
        .with_person(
            STATUS_BLIND_KEY,
            "user_2statusblind",
            afd_auth::scope::ScopeSet::from_scopes(&[afd_auth::scope::Scope::FleetWrite]),
        )
        .with_steering_queue(saved.case.base.db.clone(), saved.case.bus.clone())
        .router();
    let body = json!({"message": MESSAGE, "operation_id": FRESH_OPERATION}).to_string();
    let response = send(
        &router,
        Method::POST,
        &saved.thread,
        Some(STATUS_BLIND_KEY),
        &body,
    )
    .await;
    let status = response.status();
    let answer = json_body(response).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{answer}");
    saved.case.cleanup().await;
}
