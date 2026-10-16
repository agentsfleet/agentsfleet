//! The runner memory routes: the fleet id in the path, hydration, the recall
//! bound and capture, each refused or rendered by the handler that owns it.

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};

use super::{FLEET_ID, LEASE_ID, RUNNER_TOKEN, code_of};
use crate::harness::{Fleet, json_body, runner_id, send};

#[tokio::test]
async fn runner_memory_routes_validate_and_render() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();

    a_fleet_id_that_is_not_a_uuid_is_refused(&router).await;

    let memory_path = format!("/v1/runners/me/memory/{FLEET_ID}");
    let hydrated = send(&router, Method::GET, &memory_path, Some(RUNNER_TOKEN), "").await;
    assert_eq!(hydrated.status(), StatusCode::OK);
    // A fleet without grants hydrates in the shape every runner parses:
    // `shared` and `publish` stay off the wire.
    assert_eq!(json_body(hydrated).await, serde_json::json!({"memory": []}));

    recall_is_bounded_and_renders_both_shelves(&router, &memory_path).await;
    capture_validates_and_counts(&router, &memory_path).await;
}

/// Both verbs parse the fleet id before anything else reads the request.
async fn a_fleet_id_that_is_not_a_uuid_is_refused(router: &axum::Router) {
    for method in [Method::GET, Method::POST] {
        let malformed = send(
            router,
            method,
            "/v1/runners/me/memory/not-a-uuid",
            Some(RUNNER_TOKEN),
            "{}",
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(code_of(malformed).await, "UZ-REQ-001");
    }
}

/// A recall with no room for a result is refused; a bounded one answers both
/// the fleet's own memory and what is shared with it.
async fn recall_is_bounded_and_renders_both_shelves(router: &axum::Router, memory_path: &str) {
    let recall_path = format!("{memory_path}/recall");
    let unbounded = send(
        router,
        Method::POST,
        &recall_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":1,"query":"x","limit":0}}"#),
    )
    .await;
    assert_eq!(
        unbounded.status(),
        StatusCode::BAD_REQUEST,
        "a zero limit is refused"
    );
    let recalled = send(
        router,
        Method::POST,
        &recall_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":1,"query":"x","limit":5}}"#),
    )
    .await;
    assert_eq!(recalled.status(), StatusCode::OK);
    assert_eq!(
        json_body(recalled).await,
        serde_json::json!({"memory": [], "shared": []})
    );
}

/// A capture body missing its lease is refused; a fenced one reports what it
/// stored and skipped.
async fn capture_validates_and_counts(router: &axum::Router, memory_path: &str) {
    let malformed = send(router, Method::POST, memory_path, Some(RUNNER_TOKEN), "{}").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(malformed).await, "UZ-REQ-001");

    let captured = send(
        router,
        Method::POST,
        memory_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":7,"memory":[]}}"#),
    )
    .await;
    assert_eq!(captured.status(), StatusCode::OK);
    assert_eq!(
        json_body(captured).await,
        serde_json::json!({"stored": 0, "skipped": 0})
    );
}
