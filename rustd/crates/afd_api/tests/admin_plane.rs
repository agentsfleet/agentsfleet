//! Mounted platform administration routes through the production router.
#![cfg(feature = "test-util")]

mod harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, json_body, send};

const PLATFORM_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OPERATOR: &str = "user_platform_admin";
const MODEL_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01";
const WORKSPACE_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02";
const CREATE: &str = r#"{"provider":"anthropic","model_id":"claude-opus-5","context_cap_tokens":200000,"input_nanos_per_mtok":5,"cached_input_nanos_per_mtok":1,"output_nanos_per_mtok":25}"#;

fn fleet(scope: Scope) -> axum::Router {
    Fleet::new()
        .with_person(PLATFORM_KEY, OPERATOR, ScopeSet::from_scopes(&[scope]))
        .router()
}

#[tokio::test]
async fn admin_reads_are_mounted_behind_their_exact_scopes() {
    let models = fleet(Scope::ModelRead);
    let reached = send(
        &models,
        Method::GET,
        "/v1/admin/models",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code(reached).await, Some("UZ-INTERNAL-001"));

    let denied = send(
        &models,
        Method::GET,
        "/v1/admin/platform-keys",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let keys = fleet(Scope::PlatformKeyRead);
    let reached = send(
        &keys,
        Method::GET,
        "/v1/admin/platform-keys",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn model_mutations_validate_before_datastore_io() {
    let admin = fleet(Scope::ModelAdmin);
    for body in [
        "",
        "[]",
        r#"{"provider":"","model_id":"x","context_cap_tokens":1,"input_nanos_per_mtok":0,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#,
        r#"{"provider":"x","model_id":"x","context_cap_tokens":0,"input_nanos_per_mtok":0,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#,
    ] {
        let refused = send(
            &admin,
            Method::POST,
            "/v1/admin/models",
            Some(PLATFORM_KEY),
            body,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
        assert_eq!(code(refused).await, Some("UZ-REQ-001"));
    }

    let reached = send(
        &admin,
        Method::POST,
        "/v1/admin/models",
        Some(PLATFORM_KEY),
        CREATE,
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);

    let bad_id = send(
        &admin,
        Method::PATCH,
        "/v1/admin/models/not-a-uuid",
        Some(PLATFORM_KEY),
        r#"{"context_cap_tokens":1,"input_nanos_per_mtok":0,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#,
    )
    .await;
    assert_eq!(bad_id.status(), StatusCode::BAD_REQUEST);

    let bad_rates = send(
        &admin,
        Method::PATCH,
        &format!("/v1/admin/models/{MODEL_ID}"),
        Some(PLATFORM_KEY),
        r#"{"context_cap_tokens":1,"input_nanos_per_mtok":-1,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#,
    )
    .await;
    assert_eq!(bad_rates.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn platform_key_mutations_validate_pairing_before_io() {
    let admin = fleet(Scope::PlatformKeyAdmin);
    let unsafe_endpoint = format!(
        r#"{{"provider":"openai-compatible","source_workspace_id":"{WORKSPACE_ID}","model":"custom","base_url":"https://127.0.0.1/v1"}}"#
    );
    let refused = send(
        &admin,
        Method::PUT,
        "/v1/admin/platform-keys",
        Some(PLATFORM_KEY),
        &unsafe_endpoint,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code(refused).await, Some("UZ-PROVIDER-005"));

    let named_with_url = format!(
        r#"{{"provider":"anthropic","source_workspace_id":"{WORKSPACE_ID}","model":"claude-opus-5","base_url":"https://models.example/v1"}}"#
    );
    let refused = send(
        &admin,
        Method::PUT,
        "/v1/admin/platform-keys",
        Some(PLATFORM_KEY),
        &named_with_url,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code(refused).await, Some("UZ-PROVIDER-005"));

    let valid = format!(
        r#"{{"provider":"anthropic","source_workspace_id":"{WORKSPACE_ID}","model":"claude-opus-5","base_url":null}}"#
    );
    let reached = send(
        &admin,
        Method::PUT,
        "/v1/admin/platform-keys",
        Some(PLATFORM_KEY),
        &valid,
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);

    let bad_provider = send(
        &admin,
        Method::DELETE,
        "/v1/admin/platform-keys/abcdefghijklmnopqrstuvwxyz1234567",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(bad_provider.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn library_catalogue_routes_require_write_scope_and_validate_before_io() {
    let admin = fleet(Scope::PlatformLibraryWrite);
    let list = send(
        &admin,
        Method::GET,
        "/v1/admin/fleet-libraries",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(list.status(), StatusCode::SERVICE_UNAVAILABLE);

    for body in [
        "",
        "[]",
        r#"{"source_kind":"unknown"}"#,
        r#"{"source_kind":"github","source_ref":"owner/repo/extra"}"#,
        r#"{"source_kind":"upload","skill_markdown":"---","support_files":[{}]}"#,
    ] {
        let refused = send(
            &admin,
            Method::POST,
            "/v1/admin/fleet-libraries",
            Some(PLATFORM_KEY),
            body,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    }

    let import = send(
        &admin,
        Method::POST,
        "/v1/admin/fleet-libraries",
        Some(PLATFORM_KEY),
        r#"{"source_kind":"upload","source_ref":"unit/example","skill_markdown":"---\nname: example\ndescription: Example fleet\nversion: 1.0.0\n---\nInstructions."}"#,
    )
    .await;
    assert_eq!(import.status(), StatusCode::SERVICE_UNAVAILABLE);

    for body in [
        "",
        "[]",
        r#"{"source_repo":"owner/repo/extra"}"#,
        r#"{"required_credentials_reasons":[]}"#,
    ] {
        let refused = send(
            &admin,
            Method::PATCH,
            "/v1/admin/fleet-libraries/example",
            Some(PLATFORM_KEY),
            body,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    }

    let patch = send(
        &admin,
        Method::PATCH,
        "/v1/admin/fleet-libraries/example",
        Some(PLATFORM_KEY),
        r#"{"description":"curated"}"#,
    )
    .await;
    assert_eq!(patch.status(), StatusCode::SERVICE_UNAVAILABLE);

    let delete = send(
        &admin,
        Method::DELETE,
        "/v1/admin/fleet-libraries/example",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(delete.status(), StatusCode::SERVICE_UNAVAILABLE);

    let wrong_scope = fleet(Scope::ModelAdmin);
    let denied = send(
        &wrong_scope,
        Method::GET,
        "/v1/admin/fleet-libraries",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}

async fn code(response: axum::response::Response) -> Option<&'static str> {
    match json_body(response)
        .await
        .get("error_code")
        .and_then(serde_json::Value::as_str)
    {
        Some("UZ-INTERNAL-001") => Some("UZ-INTERNAL-001"),
        Some("UZ-REQ-001") => Some("UZ-REQ-001"),
        Some("UZ-PROVIDER-005") => Some("UZ-PROVIDER-005"),
        _other => None,
    }
}
