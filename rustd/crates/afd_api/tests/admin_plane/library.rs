//! Platform publication validates native identities and conditional edits before storage.

use super::{PLATFORM_KEY, Scope, StatusCode, code, fleet};
use crate::harness::{send, send_with_headers};
use http::{Method, header};

const COLLECTION: &str = "/v1/admin/fleet-libraries";
const ENTRY: &str = "/v1/admin/fleet-libraries/example";
const KEY: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01";
const TAG: &str = "\"native-library-one\"";
const STAGE: &str = r#"{"workspace_id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02","draft_candidate_id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d03","label":"Native example"}"#;
const ENVELOPE_BYTES_MAX: usize = afd_wire::authoring::ENVELOPE_BYTES_MAX;

#[tokio::test]
async fn publication_requires_scope_and_a_replay_key_before_storage() {
    let admin = fleet(Scope::PlatformLibraryWrite);
    let anonymous = send(&admin, Method::POST, COLLECTION, None, STAGE).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let wrong_scope = send(
        &fleet(Scope::ModelAdmin),
        Method::POST,
        COLLECTION,
        Some(PLATFORM_KEY),
        STAGE,
    )
    .await;
    assert_eq!(wrong_scope.status(), StatusCode::FORBIDDEN);
    let missing = send(&admin, Method::POST, COLLECTION, Some(PLATFORM_KEY), STAGE).await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    let reached = send_with_headers(
        &admin,
        Method::POST,
        COLLECTION,
        Some(PLATFORM_KEY),
        STAGE,
        &[(header::HeaderName::from_static("idempotency-key"), KEY)],
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn retired_inputs_and_oversized_native_bodies_are_refused_before_storage() {
    let admin = fleet(Scope::PlatformLibraryWrite);
    for body in [
        "",
        "[]",
        r#"{"source_kind":"upload","skill_markdown":"private"}"#,
        r#"{"workspace_id":"bad","draft_candidate_id":"bad","label":"Example"}"#,
    ] {
        let refused = send_with_headers(
            &admin,
            Method::POST,
            COLLECTION,
            Some(PLATFORM_KEY),
            body,
            &[(header::HeaderName::from_static("idempotency-key"), KEY)],
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    }
    let exact = format!("{STAGE}{}", " ".repeat(ENVELOPE_BYTES_MAX - STAGE.len()));
    let reached = send_with_headers(
        &admin,
        Method::POST,
        COLLECTION,
        Some(PLATFORM_KEY),
        &exact,
        &[(header::HeaderName::from_static("idempotency-key"), KEY)],
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
    let oversized = format!("{exact} ");
    let refused = send_with_headers(
        &admin,
        Method::POST,
        COLLECTION,
        Some(PLATFORM_KEY),
        &oversized,
        &[(header::HeaderName::from_static("idempotency-key"), KEY)],
    )
    .await;
    assert_eq!(refused.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(code(refused).await, Some("UZ-REQ-002"));
}

#[tokio::test]
async fn platform_edits_and_removal_require_the_inspected_resource_tag() {
    let admin = fleet(Scope::PlatformLibraryWrite);
    for method in [Method::PATCH, Method::DELETE] {
        let missing = send(
            &admin,
            method.clone(),
            ENTRY,
            Some(PLATFORM_KEY),
            r#"{"description":"curated"}"#,
        )
        .await;
        assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
        let reached = send_with_headers(
            &admin,
            method,
            ENTRY,
            Some(PLATFORM_KEY),
            r#"{"description":"curated"}"#,
            &[(header::IF_MATCH, TAG)],
        )
        .await;
        assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
    for body in [
        "",
        "[]",
        r#"{"source_repo":"owner/repo"}"#,
        r#"{"required_credentials_reasons":[]}"#,
    ] {
        let refused = send_with_headers(
            &admin,
            Method::PATCH,
            ENTRY,
            Some(PLATFORM_KEY),
            body,
            &[(header::IF_MATCH, TAG)],
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    }
}
