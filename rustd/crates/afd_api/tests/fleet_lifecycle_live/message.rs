//! Successful message and public-library routes over the parent live fixture.

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
