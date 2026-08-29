//! Parsing and response shapes owned by the small runner handlers.

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};

use super::{FLEET_ID, LEASE_ID, RUNNER_TOKEN, code_of};
use crate::harness::{Fleet, json_body, runner_id, send};

#[tokio::test]
async fn runner_lease_satellite_routes_parse_and_render() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();

    let renew_path = format!("/v1/runners/me/leases/{LEASE_ID}/renew");
    for body in [
        "",
        "not-json",
        r#"{"input_tokens":1,"cached_input_tokens":2,"output_tokens":3}"#,
    ] {
        let renewed = send(&router, Method::POST, &renew_path, Some(RUNNER_TOKEN), body).await;
        assert_eq!(renewed.status(), StatusCode::OK);
        let body = json_body(renewed).await;
        assert_eq!(
            body.get("lease_expires_at"),
            Some(&serde_json::json!(1_760_000_000_000_i64))
        );
    }

    let mint_path = "/v1/runners/me/credentials/mint";
    let malformed = send(&router, Method::POST, mint_path, Some(RUNNER_TOKEN), "{}").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(malformed).await, "UZ-REQ-001");

    let unconfigured = send(
        &router,
        Method::POST,
        mint_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","integration":"github","scope":null}}"#),
    )
    .await;
    assert_eq!(unconfigured.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code_of(unconfigured).await, "UZ-CRED-002");
}

#[tokio::test]
async fn runner_memory_routes_validate_and_render() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();

    for method in [Method::GET, Method::POST] {
        let malformed = send(
            &router,
            method,
            "/v1/runners/me/memory/not-a-uuid",
            Some(RUNNER_TOKEN),
            "{}",
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(code_of(malformed).await, "UZ-REQ-001");
    }

    let memory_path = format!("/v1/runners/me/memory/{FLEET_ID}");
    let hydrated = send(&router, Method::GET, &memory_path, Some(RUNNER_TOKEN), "").await;
    assert_eq!(hydrated.status(), StatusCode::OK);
    assert_eq!(json_body(hydrated).await, serde_json::json!({"memory": []}));

    let malformed = send(
        &router,
        Method::POST,
        &memory_path,
        Some(RUNNER_TOKEN),
        "{}",
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(malformed).await, "UZ-REQ-001");

    let captured = send(
        &router,
        Method::POST,
        &memory_path,
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
