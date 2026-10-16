//! A message's operation id across a stop and across a workspace boundary, on
//! a fleet created through Save.

use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::super::super::{json_body, send};
use super::Saved;

/// The operation id a client repeats across its retries.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0b01";

/// A second operation id, for new work the stopped fleet must still refuse.
const FRESH_OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0b02";

/// The message the operation first carried.
const MESSAGE: &str = "roll back staging";

fn code(body: &Value) -> Option<&str> {
    body.get("error_code").and_then(Value::as_str)
}

/// A fleet saved with no trigger is one only ever talked to: Save stores its
/// set as empty, and a steer is admitted all the same, because nothing on the
/// message path reads what a fleet declares may wake it.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
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
        admitted["event_id"]
            .as_str()
            .is_some_and(|event| !event.is_empty()),
        "a steer to a trigger-less fleet is admitted as an event: {admitted}"
    );

    saved.case.cleanup().await;
}

/// The 202 says whether it answered an earlier send: `false` for the send that
/// admitted the message, `true` for a repeat while the fleet runs and after it
/// stopped.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_steer_202_names_a_replay() {
    let saved = Saved::create().await;
    let replayed = |body: &Value| body.get("replayed").and_then(Value::as_bool);

    let (_, fresh) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(replayed(&fresh), Some(false), "{fresh}");
    let (_, repeat) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(replayed(&repeat), Some(true), "{repeat}");
    saved.stop().await;
    let (_, stopped) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(replayed(&stopped), Some(true), "{stopped}");
    assert_eq!(fresh.get("event_id"), stopped.get("event_id"));

    saved.case.cleanup().await;
}

/// A retry of an admitted message is answered with its event even after the
/// fleet stops taking work; new work and a changed message are refused.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_replay_bypasses_ingress_refusal() {
    let saved = Saved::create().await;

    let (status, first) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");
    let event = first["event_id"]
        .as_str()
        .expect("a 202 names its event")
        .to_owned();
    saved.stop().await;

    let (status, retried) = saved.steer(&saved.thread, OPERATION, MESSAGE).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "answered, stopped or not: {retried}"
    );
    assert_eq!(retried["event_id"].as_str(), Some(event.as_str()));

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
        Some(error_code::AGENTSFLEET_PAUSED_INGRESS.as_str())
    );

    assert_keyless_message_refused(&saved).await;

    saved.case.cleanup().await;
}

/// Without an id there is no earlier admission to answer: new work, refused.
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
        Some(error_code::AGENTSFLEET_PAUSED_INGRESS.as_str())
    );
}

/// A guessed operation id on a fleet this workspace does not hold is a 404,
/// the same answer an unknown fleet gets — never a 409 that would tell the
/// caller the id exists in another tenant's ledger.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
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
