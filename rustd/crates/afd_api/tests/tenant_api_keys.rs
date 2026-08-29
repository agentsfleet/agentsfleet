//! The tenant API-key routes through the real router and production store.
//!
//! The fixture deliberately has no Postgres behind it. A request that reaches
//! the store therefore answers its real unavailable error, which distinguishes
//! a valid request from one rejected by authentication, scope, path, paging or
//! body parsing without introducing a second implementation of the key store.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

const KEYS: &str = "/v1/api-keys";
const KEY: &str = "/v1/api-keys/0195b4ba-8d3a-7f13-8abc-2b3e1e0f7042";
const BAD_KEY: &str = "/v1/api-keys/not-an-id";
const CREDENTIAL: &str = "agt_t42abcdef42abcdef42abcdef42abcdef42abcdef42abcdef42abcdef42abcdef";
const SUBJECT: &str = "user_2apikeys";
const READ: ScopeSet = ScopeSet::from_scopes(&[Scope::ApikeyRead]);
const WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::ApikeyWrite]);
const ADMIN: ScopeSet = ScopeSet::from_scopes(&[Scope::ApikeyAdmin]);
const NONE: ScopeSet = ScopeSet::from_scopes(&[]);

async fn send(
    scopes: ScopeSet,
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(CREDENTIAL, SUBJECT, scopes)
        .router();
    harness::send(&router, method, path, credential, body).await
}

async fn detail_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("a refusal carries detail")
        .to_owned()
}

async fn assert_store_reached(response: axum::response::Response, case: &str) {
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{case}: the production store is the first unavailable dependency"
    );
    assert_eq!(
        detail_of(response).await,
        afd_tenant::error::detail::DETAIL_DATABASE_UNAVAILABLE,
        "{case}: the refusal belongs to the store"
    );
}

#[tokio::test]
async fn every_api_key_verb_requires_a_credential() {
    for (method, path, body) in [
        (Method::GET, KEYS, ""),
        (Method::POST, KEYS, r#"{"key_name":"deploy"}"#),
        (Method::PATCH, KEY, r#"{"active":false}"#),
        (Method::DELETE, KEY, ""),
    ] {
        let response = send(ADMIN, method, path, None, body).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn each_verb_enforces_its_scope_before_the_store() {
    for (method, path, body) in [
        (Method::GET, KEYS, ""),
        (Method::POST, KEYS, r#"{"key_name":"deploy"}"#),
        (Method::PATCH, KEY, r#"{"active":false}"#),
        (Method::DELETE, KEY, ""),
    ] {
        let response = send(NONE, method, path, Some(CREDENTIAL), body).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
    }
}

#[tokio::test]
async fn valid_requests_reach_the_production_store() {
    let cases = [
        (READ, Method::GET, KEYS, "", "list"),
        (
            WRITE,
            Method::POST,
            KEYS,
            r#"{"key_name":"deploy","description":"release automation"}"#,
            "mint",
        ),
        (WRITE, Method::PATCH, KEY, r#"{"active":false}"#, "revoke"),
        (ADMIN, Method::DELETE, KEY, "", "delete"),
    ];
    for (scopes, method, path, body, case) in cases {
        let response = send(scopes, method, path, Some(CREDENTIAL), body).await;
        assert_store_reached(response, case).await;
    }
}

#[tokio::test]
async fn mint_rejects_unreadable_and_invalid_bodies_before_tenant_lookup() {
    let too_long = format!(
        r#"{{"key_name":"deploy","description":"{}"}}"#,
        "d".repeat(257)
    );
    for body in [
        "{not json".to_owned(),
        r#"{"key_name":""}"#.to_owned(),
        r#"{"key_name":"contains spaces"}"#.to_owned(),
        too_long,
    ] {
        let response = send(WRITE, Method::POST, KEYS, Some(CREDENTIAL), &body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn revoke_requires_an_identifier_and_the_one_supported_intent() {
    let bad_id = send(
        WRITE,
        Method::PATCH,
        BAD_KEY,
        Some(CREDENTIAL),
        r#"{"active":false}"#,
    )
    .await;
    assert_eq!(bad_id.status(), StatusCode::BAD_REQUEST);
    assert_eq!(detail_of(bad_id).await, "id must be a valid UUIDv7");

    for body in ["{not json", r"{}"] {
        let response = send(WRITE, Method::PATCH, KEY, Some(CREDENTIAL), body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    let unsupported = send(
        WRITE,
        Method::PATCH,
        KEY,
        Some(CREDENTIAL),
        r#"{"active":true}"#,
    )
    .await;
    assert_eq!(unsupported.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn delete_rejects_an_invalid_identifier_before_tenant_lookup() {
    let response = send(ADMIN, Method::DELETE, BAD_KEY, Some(CREDENTIAL), "").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(detail_of(response).await, "id must be a valid UUIDv7");
}

#[tokio::test]
async fn list_refuses_bad_page_controls_and_accepts_its_boundaries() {
    for wrong in ["0", "101", "abc", "-1"] {
        let path = format!("{KEYS}?limit={wrong}");
        let response = send(READ, Method::GET, &path, Some(CREDENTIAL), "").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }

    for accepted in [
        format!("{KEYS}?limit=1"),
        format!("{KEYS}?limit=100&sort=created_at"),
        format!("{KEYS}?sort=key_name"),
    ] {
        let response = send(READ, Method::GET, &accepted, Some(CREDENTIAL), "").await;
        assert_store_reached(response, &accepted).await;
    }
}
