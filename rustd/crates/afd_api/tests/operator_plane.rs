//! Operator-route guards and mounting through the production router.
#![cfg(feature = "test-util")]

mod harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, json_body, send};

const TENANT_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OPERATOR: &str = "user_operator_surface";
const RUNNER: &str = "019329c5-0000-7000-8000-0000000000a1";

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

#[tokio::test]
async fn runner_patch_is_mounted_behind_runner_write_and_rejects_shape_before_io() {
    let reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::StreamRead]),
        )
        .router();
    let denied = send(
        &reader,
        Method::PATCH,
        &format!("/v1/fleets/runners/{RUNNER}"),
        Some(TENANT_KEY),
        r#"{"action":"cordon"}"#,
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let writer = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerWrite]),
        )
        .router();
    for body in [
        "",
        r"{}",
        r#"{"action":"cordon","assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":1,"extra_binds":[]}}"#,
        r#"{"action":"not_an_action"}"#,
    ] {
        let malformed = send(
            &writer,
            Method::PATCH,
            &format!("/v1/fleets/runners/{RUNNER}"),
            Some(TENANT_KEY),
            body,
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_body(malformed)
                .await
                .get("error_code")
                .and_then(serde_json::Value::as_str),
            Some("UZ-REQ-001")
        );
    }

    let unsafe_policy = send(
        &writer,
        Method::PATCH,
        &format!("/v1/fleets/runners/{RUNNER}"),
        Some(TENANT_KEY),
        r#"{"assigned_policy":{"sandbox_tier":"landlock_full","network_policy":"allow_list_egress","registry_allowlist":[],"worker_count":1,"extra_binds":[{"path":"/etc","mode":"read_write","note":"unsafe"}]}}"#,
    )
    .await;
    assert_eq!(unsafe_policy.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(unsafe_policy)
            .await
            .get("error_code")
            .and_then(serde_json::Value::as_str),
        Some("UZ-REQ-001")
    );

    let reached = send(
        &writer,
        Method::PATCH,
        &format!("/v1/fleets/runners/{RUNNER}"),
        Some(TENANT_KEY),
        r#"{"action":"cordon"}"#,
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}
