//! Successful message and public-library routes over the parent live fixture,
//! and a steer's operation id across a pause.

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
/// even after the fleet pauses; new work and a changed message are refused.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_replay_bypasses_paused_ingress() {
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

    let paused = send(
        &router,
        Method::PATCH,
        &item,
        Some(&fixture.token),
        &serde_json::json!({ "status": "paused" }).to_string(),
    )
    .await;
    assert_eq!(paused.status(), StatusCode::OK);

    let (status, retried) = steer(&router, &fixture, &thread, OPERATION, MESSAGE).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "a retry of an admitted message is answered, paused or not: {retried}"
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

    fixture.cleanup().await;
}
