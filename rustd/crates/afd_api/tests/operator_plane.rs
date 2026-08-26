//! Operator-route guards and mounting through the production router.
#![cfg(feature = "test-util")]

mod harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, json_body, send};

const TENANT_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OPERATOR: &str = "user_operator_surface";

#[tokio::test]
async fn operator_reads_are_mounted_behind_their_exact_scopes() {
    let runner_reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerRead]),
        )
        .router();
    let reached = send(
        &runner_reader,
        Method::GET,
        "/v1/fleets/runners",
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(reached)
            .await
            .get("error_code")
            .and_then(serde_json::Value::as_str),
        Some("UZ-INTERNAL-001")
    );

    let denied = send(
        &runner_reader,
        Method::GET,
        "/v1/fleets/streams",
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let stream_reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::StreamRead]),
        )
        .router();
    let overview = send(
        &stream_reader,
        Method::GET,
        "/v1/fleets/streams",
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(overview.status(), StatusCode::OK);
    assert_eq!(
        json_body(overview).await,
        serde_json::json!({"items": [], "total": 0, "max_streams": 64})
    );
}
