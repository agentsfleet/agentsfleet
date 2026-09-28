//! Successful message and public-library routes over the parent live fixture,
//! and a steer's operation id across a stop and across a workspace boundary.

use afd_core::error_code;
use afd_core::id::Uuid7;
use http::{Method, StatusCode};
use serde_json::Value;

use super::{Fixture, json_body, send};

pub(super) async fn exercise(
    router: &axum::Router,
    fixture: &Fixture,
    workspace: &str,
    fleet: &Uuid7,
) {
    let thread = format!("{workspace}/fleets/{}/messages", fleet.as_str());
    let listed = send(router, Method::GET, &thread, Some(&fixture.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    assert_eq!(
        listed.pointer("/items/0/event_id").and_then(Value::as_str),
        Some(super::EVENT)
    );

    let steered = send(
        router,
        Method::POST,
        &thread,
        Some(&fixture.token),
        r#"{"message":"ship the next change"}"#,
    )
    .await;
    assert_eq!(steered.status(), StatusCode::ACCEPTED);
    assert_eq!(
        json_body(steered)
            .await
            .get("status")
            .and_then(Value::as_str),
        Some("accepted")
    );

    let bundles = send(
        router,
        Method::GET,
        "/v1/fleets/bundles",
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(bundles.status(), StatusCode::OK);
    let bundles = json_body(bundles).await;
    assert!(
        bundles
            .get("items")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("id").and_then(Value::as_str) == Some(fixture.library.as_str())
                })
            }),
        "the seeded public library is present in the bundle catalog: {bundles}"
    );
}

/// The operation id a client repeats across its retries.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0b01";

/// A second operation id, for new work the paused fleet must still refuse.
const FRESH_OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0b02";

/// The message the operation first carried.
const MESSAGE: &str = "roll back staging";

/// One steer naming its operation, answered with the status and the body.
async fn steer(
    router: &axum::Router,
    fixture: &Fixture,
    thread: &str,
    operation: &str,
    message: &str,
) -> (StatusCode, Value) {
    let body = serde_json::json!({ "message": message, "operation_id": operation }).to_string();
    let response = send(router, Method::POST, thread, Some(&fixture.token), &body).await;
    let status = response.status();
    (status, json_body(response).await)
}

/// Dimension 4.3 — a retry of an admitted steer is answered with its event
/// even after the fleet stops taking work; new work and a changed message are
/// refused.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_replay_bypasses_ingress_refusal() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = super::live_router(&fixture).await;
    let workspace = format!("/v1/workspaces/{}", fixture.workspace.as_str());
    let fleet = super::install(&router, &fixture, &workspace).await;
    let item = format!("{workspace}/fleets/{}", fleet.as_str());
    let thread = format!("{item}/messages");

    let (status, first) = steer(&router, &fixture, &thread, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");
    let event = first
        .get("event_id")
        .and_then(Value::as_str)
        .expect("a 202 names its event")
        .to_owned();

    let stopped = send(
        &router,
        Method::PATCH,
        &item,
        Some(&fixture.token),
        // `paused` is the anomaly gate's to set; an operator stops a fleet,
        // and a stopped fleet meets the same ingress refusal.
        &serde_json::json!({ "status": "stopped" }).to_string(),
    )
    .await;
    assert_eq!(stopped.status(), StatusCode::OK);

    let (status, retried) = steer(&router, &fixture, &thread, OPERATION, MESSAGE).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "a retry of an admitted message is answered, stopped or not: {retried}"
    );
    assert_eq!(
        retried.get("event_id").and_then(Value::as_str),
        Some(event.as_str())
    );

    let (status, changed) = steer(
        &router,
        &fixture,
        &thread,
        OPERATION,
        "roll back production",
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{changed}");
    assert_eq!(
        changed.get("error_code").and_then(Value::as_str),
        Some(error_code::AGENTSFLEET_OPERATION_CONFLICT.as_str())
    );
    assert_eq!(
        changed.get("current_state").and_then(Value::as_str),
        Some("admitted")
    );

    let (status, fresh) = steer(&router, &fixture, &thread, FRESH_OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::CONFLICT, "{fresh}");
    assert_eq!(
        fresh.get("error_code").and_then(Value::as_str),
        Some(error_code::AGENTSFLEET_PAUSED_INGRESS.as_str())
    );

    // Without an id there is no earlier admission to answer: new work, refused.
    let keyless = serde_json::json!({ "message": MESSAGE }).to_string();
    let unnamed = send(
        &router,
        Method::POST,
        &thread,
        Some(&fixture.token),
        &keyless,
    )
    .await;
    assert_eq!(unnamed.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(unnamed)
            .await
            .get("error_code")
            .and_then(Value::as_str),
        Some(error_code::AGENTSFLEET_PAUSED_INGRESS.as_str())
    );

    fixture.cleanup().await;
}

/// A guessed operation id on a fleet this workspace does not hold is a 404,
/// the same answer an unknown fleet gets — never a 409 that would tell the
/// caller the id exists in another tenant's ledger.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_foreign_fleet_replay_is_not_found() {
    let owner = Fixture::create().await;
    owner.seed().await;
    let owner_router = super::live_router(&owner).await;
    let owner_workspace = format!("/v1/workspaces/{}", owner.workspace.as_str());
    let fleet = super::install(&owner_router, &owner, &owner_workspace).await;
    let owned_thread = format!("{owner_workspace}/fleets/{}/messages", fleet.as_str());
    let (status, first) = steer(&owner_router, &owner, &owned_thread, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");

    // Another tenant, naming the owner's fleet under its own workspace, with
    // the same id and the same message.
    let prober = Fixture::create().await;
    prober.seed().await;
    let prober_router = super::live_router(&prober).await;
    let probe = format!(
        "/v1/workspaces/{}/fleets/{}/messages",
        prober.workspace.as_str(),
        fleet.as_str()
    );
    let (status, answer) = steer(&prober_router, &prober, &probe, OPERATION, MESSAGE).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");
    assert_eq!(
        answer.get("error_code").and_then(Value::as_str),
        Some(error_code::AGENTSFLEET_NOT_FOUND.as_str())
    );

    prober.cleanup().await;
    owner.cleanup().await;
}
