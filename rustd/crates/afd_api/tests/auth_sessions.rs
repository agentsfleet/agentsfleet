//! Device-flow requests through the production router with network seams closed.
//!
//! Open, poll and verify are intentionally unauthenticated. Approve and cancel
//! require a verified dashboard session, which the fixture supplies through the
//! real OIDC authentication path with only the key-set verifier replaced. The
//! Dragonfly store is production code over an unreachable lazy connection, so 503
//! proves a well-formed request reached the service boundary.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::ScopeSet;
use afd_core::error_code;
use http::{Method, StatusCode};

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
            "{path}: a valid open request reaches the unavailable Dragonfly store"
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

/// The registry code a refusal carries.
async fn code_of(response: axum::response::Response) -> Option<String> {
    harness::json_body(response)
        .await
        .get("error_code")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[tokio::test]
async fn test_session_fields_keep_their_auth_codes() {
    // The bounds moved onto garde structs; each field still answers its own
    // registry code one byte past its bound, which is what a client branches on.
    let open = |public_key: &str, token_name: &str| {
        format!(r#"{{"public_key":"{public_key}","token_name":"{token_name}"}}"#)
    };
    let approve = |ciphertext: &str, nonce: &str, code: &str| {
        format!(
            r#"{{"dashboard_public_key":"k","ciphertext":"{ciphertext}","nonce":"{nonce}","verification_code":"{code}"}}"#
        )
    };
    let opened = [
        (
            open(&"k".repeat(201), "laptop"),
            error_code::INVALID_PUBLIC_KEY,
        ),
        (open("k", &"t".repeat(65)), error_code::INVALID_TOKEN_NAME),
    ];
    for (body, expected) in opened {
        let response = open_router(Method::POST, SESSIONS, None, &body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{expected:?}");
        assert_eq!(code_of(response).await.as_deref(), Some(expected.as_str()));
    }
    let approved = [
        (
            approve(&"c".repeat(4097), "n", "012345"),
            error_code::INVALID_CIPHERTEXT,
        ),
        (
            approve("c", &"n".repeat(33), "012345"),
            error_code::INVALID_NONCE,
        ),
        (
            approve("c", "n", "0123456"),
            error_code::INVALID_VERIFICATION_CODE,
        ),
    ];
    for (body, expected) in approved {
        let response = dashboard_router(Method::PATCH, APPROVE, Some(DASHBOARD_TOKEN), &body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{expected:?}");
        assert_eq!(code_of(response).await.as_deref(), Some(expected.as_str()));
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
