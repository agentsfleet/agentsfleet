//! Device-flow requests through the production router with network seams closed.
//!
//! Open, poll and verify are intentionally unauthenticated. Approve and cancel
//! require a verified dashboard session, which the fixture supplies through the
//! real OIDC authentication path with only the key-set verifier replaced. The
//! Redis store is production code over an unreachable lazy connection, so 503
//! proves a well-formed request reached the service boundary.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::ScopeSet;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

const SESSIONS: &str = "/v1/auth/sessions";
const SESSION: &str = "/v1/auth/sessions/0195b4ba-8d3a-7f13-8abc-2b3e1e0f7051";
const APPROVE: &str = "/v1/auth/sessions/0195b4ba-8d3a-7f13-8abc-2b3e1e0f7051/approve";
const VERIFY: &str = "/v1/auth/sessions/0195b4ba-8d3a-7f13-8abc-2b3e1e0f7051/verify";
const ALL: &str = "/v1/auth/sessions/all";
const DASHBOARD_TOKEN: &str = "fixture.header.payload";
const TENANT_KEY: &str = "agt_t5151515151515151515151515151515151515151515151515151515151515151";
const SUBJECT: &str = "user_2device_flow";
const OPEN_BODY: &str = r#"{"public_key":"fixture-public-key","token_name":"laptop"}"#;
const APPROVE_BODY: &str = r#"{
  "dashboard_public_key":"dashboard-key",
  "ciphertext":"sealed-credential",
  "nonce":"fixture-nonce",
  "verification_code":"012345"
}"#;
const VERIFY_BODY: &str = r#"{"verification_code":"012345"}"#;

async fn open_router(
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> axum::response::Response {
    harness::send(&Fleet::new().router(), method, path, credential, body).await
}

async fn dashboard_router(
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let router = Fleet::new().with_dashboard(SUBJECT).router();
    harness::send(&router, method, path, credential, body).await
}

#[tokio::test]
async fn open_poll_and_verify_need_no_bearer_but_reach_the_queue() {
    for (method, path, body) in [
        (Method::POST, SESSIONS, OPEN_BODY),
        (Method::GET, SESSION, ""),
        (Method::POST, VERIFY, VERIFY_BODY),
    ] {
        let response = open_router(method, path, None, body).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{path}: a valid open request reaches the unavailable Redis store"
        );
    }
}

#[tokio::test]
async fn opening_refuses_unreadable_or_unbounded_input_before_redis() {
    let oversized_key = format!(
        r#"{{"public_key":"{}","token_name":"laptop"}}"#,
        "k".repeat(201)
    );
    for body in [
        "{not json".to_owned(),
        r#"{"public_key":"","token_name":"laptop"}"#.to_owned(),
        r#"{"public_key":"key","token_name":"line\nbreak"}"#.to_owned(),
        oversized_key,
    ] {
        let response = open_router(Method::POST, SESSIONS, None, &body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn verification_refuses_unreadable_and_wrong_shaped_codes_before_redis() {
    for body in [
        "{not json",
        r#"{"verification_code":"12345"}"#,
        r#"{"verification_code":"12345a"}"#,
    ] {
        let response = open_router(Method::POST, VERIFY, None, body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn dashboard_mutations_require_a_verified_session_class() {
    for (method, path, body) in [
        (Method::PATCH, APPROVE, APPROVE_BODY),
        (Method::DELETE, SESSION, ""),
        (Method::DELETE, ALL, ""),
    ] {
        let missing = dashboard_router(method.clone(), path, None, body).await;
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED, "{path}");

        let tenant_router = Fleet::new()
            .with_person(TENANT_KEY, SUBJECT, ScopeSet::EMPTY)
            .router();
        let wrong_class = harness::send(&tenant_router, method, path, Some(TENANT_KEY), body).await;
        assert_eq!(wrong_class.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn a_dashboard_session_reaches_each_mutation_service() {
    for (method, path, body) in [
        (Method::PATCH, APPROVE, APPROVE_BODY),
        (Method::DELETE, SESSION, ""),
        (Method::DELETE, ALL, ""),
    ] {
        let response = dashboard_router(method, path, Some(DASHBOARD_TOKEN), body).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
    }
}

#[tokio::test]
async fn approval_fields_are_parsed_after_dashboard_authentication() {
    for body in [
        "{not json",
        r#"{"dashboard_public_key":"","ciphertext":"c","nonce":"n","verification_code":"012345"}"#,
        r#"{"dashboard_public_key":"k","ciphertext":"","nonce":"n","verification_code":"012345"}"#,
        r#"{"dashboard_public_key":"k","ciphertext":"c","nonce":"","verification_code":"012345"}"#,
        r#"{"dashboard_public_key":"k","ciphertext":"c","nonce":"n","verification_code":"wrong"}"#,
    ] {
        let response = dashboard_router(Method::PATCH, APPROVE, Some(DASHBOARD_TOKEN), body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn device_sessions_open_approve_verify_replay_and_cancel() {
    let router = Fleet::new()
        .with_session_queue(harness::connect_redis().await)
        .with_dashboard(SUBJECT)
        .router();

    let redeemable = open_session(&router, "redeemable").await;
    assert_session_status(&router, &redeemable, "pending").await;
    approve_session(&router, &redeemable).await;
    assert_session_status(&router, &redeemable, "verification_pending").await;
    for _attempt in 0..2 {
        let verified = harness::send(
            &router,
            Method::POST,
            &format!("/v1/auth/sessions/{redeemable}/verify"),
            None,
            VERIFY_BODY,
        )
        .await;
        assert_eq!(verified.status(), StatusCode::OK);
        let body = harness::json_body(verified).await;
        assert_eq!(
            body.get("ciphertext").and_then(Value::as_str),
            Some("sealed-credential")
        );
    }

    let cancelled = open_session(&router, "cancelled").await;
    approve_session(&router, &cancelled).await;
    for _attempt in 0..2 {
        let response = harness::send(
            &router,
            Method::DELETE,
            &format!("/v1/auth/sessions/{cancelled}"),
            Some(DASHBOARD_TOKEN),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    for name in ["bulk-one", "bulk-two"] {
        let session = open_session(&router, name).await;
        approve_session(&router, &session).await;
    }
    let all = harness::send(&router, Method::DELETE, ALL, Some(DASHBOARD_TOKEN), "").await;
    assert_eq!(all.status(), StatusCode::OK);
    assert_eq!(
        harness::json_body(all)
            .await
            .get("aborted_count")
            .and_then(Value::as_u64),
        Some(2)
    );
}

async fn open_session(router: &axum::Router, token_name: &str) -> String {
    let response = harness::send(
        router,
        Method::POST,
        SESSIONS,
        None,
        &format!(r#"{{"public_key":"fixture-public-key","token_name":"{token_name}"}}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    harness::json_body(response)
        .await
        .get("session_id")
        .and_then(Value::as_str)
        .expect("an opened session returns its id")
        .to_owned()
}

async fn approve_session(router: &axum::Router, session: &str) {
    let response = harness::send(
        router,
        Method::PATCH,
        &format!("/v1/auth/sessions/{session}/approve"),
        Some(DASHBOARD_TOKEN),
        APPROVE_BODY,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn assert_session_status(router: &axum::Router, session: &str, expected: &str) {
    let response = harness::send(
        router,
        Method::GET,
        &format!("/v1/auth/sessions/{session}"),
        None,
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        harness::json_body(response)
            .await
            .get("status")
            .and_then(Value::as_str),
        Some(expected)
    );
}
