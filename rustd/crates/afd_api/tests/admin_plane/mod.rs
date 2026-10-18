//! Mounted platform administration routes through the production router.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, json_body, send};

const PLATFORM_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OPERATOR: &str = "user_platform_admin";
const MODEL_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01";
const WORKSPACE_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02";
/// A complete create body: evidence and one open-ended base tier.
const CREATE: &str = r#"{"provider":"anthropic","model_id":"claude-opus-5","metadata":{"display_name":"Opus 5","context_cap_tokens":200000,"context_source_url":"https://example.test/models","context_verified_at":"2026-10-18T00:00:00Z","advanced":false},"prices":[{"effective_from":"2026-10-01T00:00:00Z","effective_until":null,"prompt_tokens_above":0,"input_nanos_per_mtok":"5","cached_input_nanos_per_mtok":"1","cache_write_nanos_per_mtok":null,"cache_write_5m_nanos_per_mtok":null,"cache_write_1h_nanos_per_mtok":null,"output_nanos_per_mtok":"25","source_url":"https://example.test/pricing","verified_at":"2026-10-18T00:00:00Z","conditions":""}]}"#;
/// The same model as a complete patch.
const PATCH: &str = r#"{"metadata":{"display_name":"Opus 5","context_cap_tokens":200000,"context_source_url":"https://example.test/models","context_verified_at":"2026-10-18T00:00:00Z","advanced":false},"prices":[{"effective_from":"2026-10-01T00:00:00Z","effective_until":null,"prompt_tokens_above":0,"input_nanos_per_mtok":"5","cached_input_nanos_per_mtok":"1","cache_write_nanos_per_mtok":null,"cache_write_5m_nanos_per_mtok":null,"cache_write_1h_nanos_per_mtok":null,"output_nanos_per_mtok":"25","source_url":"https://example.test/pricing","verified_at":"2026-10-18T00:00:00Z","conditions":""}]}"#;
/// A create whose provider is empty.
const EMPTY_PROVIDER: &str = r#"{"provider":"","model_id":"claude-opus-5","metadata":{"display_name":"Opus 5","context_cap_tokens":200000,"context_source_url":"https://example.test/models","context_verified_at":"2026-10-18T00:00:00Z","advanced":false},"prices":[{"effective_from":"2026-10-01T00:00:00Z","effective_until":null,"prompt_tokens_above":0,"input_nanos_per_mtok":"5","cached_input_nanos_per_mtok":"1","cache_write_nanos_per_mtok":null,"cache_write_5m_nanos_per_mtok":null,"cache_write_1h_nanos_per_mtok":null,"output_nanos_per_mtok":"25","source_url":"https://example.test/pricing","verified_at":"2026-10-18T00:00:00Z","conditions":""}]}"#;
/// A create whose context cap is zero.
const ZERO_CAP: &str = r#"{"provider":"x","model_id":"claude-opus-5","metadata":{"display_name":"Opus 5","context_cap_tokens":0,"context_source_url":"https://example.test/models","context_verified_at":"2026-10-18T00:00:00Z","advanced":false},"prices":[{"effective_from":"2026-10-01T00:00:00Z","effective_until":null,"prompt_tokens_above":0,"input_nanos_per_mtok":"5","cached_input_nanos_per_mtok":"1","cache_write_nanos_per_mtok":null,"cache_write_5m_nanos_per_mtok":null,"cache_write_1h_nanos_per_mtok":null,"output_nanos_per_mtok":"25","source_url":"https://example.test/pricing","verified_at":"2026-10-18T00:00:00Z","conditions":""}]}"#;
/// A patch whose base rate is negative.
const NEGATIVE_RATE: &str = r#"{"metadata":{"display_name":"Opus 5","context_cap_tokens":200000,"context_source_url":"https://example.test/models","context_verified_at":"2026-10-18T00:00:00Z","advanced":false},"prices":[{"effective_from":"2026-10-01T00:00:00Z","effective_until":null,"prompt_tokens_above":0,"input_nanos_per_mtok":"-1","cached_input_nanos_per_mtok":"1","cache_write_nanos_per_mtok":null,"cache_write_5m_nanos_per_mtok":null,"cache_write_1h_nanos_per_mtok":null,"output_nanos_per_mtok":"25","source_url":"https://example.test/pricing","verified_at":"2026-10-18T00:00:00Z","conditions":""}]}"#;

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
    for body in ["", "[]", EMPTY_PROVIDER, ZERO_CAP] {
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

    a_model_id_that_is_not_a_uuid_is_refused(&admin).await;

    let bad_rates = send(
        &admin,
        Method::PATCH,
        &format!("/v1/admin/models/{MODEL_ID}"),
        Some(PLATFORM_KEY),
        NEGATIVE_RATE,
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
    refused_as_an_unusable_pairing(&admin, &unsafe_endpoint).await;

    let named_with_url = format!(
        r#"{{"provider":"anthropic","source_workspace_id":"{WORKSPACE_ID}","model":"claude-opus-5","base_url":"https://models.example/v1"}}"#
    );
    refused_as_an_unusable_pairing(&admin, &named_with_url).await;

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

/// Both model verbs that name an id parse it before anything else, and the
/// delete says what a usable id is.
async fn a_model_id_that_is_not_a_uuid_is_refused(admin: &axum::Router) {
    let bad_id = send(
        admin,
        Method::PATCH,
        "/v1/admin/models/not-a-uuid",
        Some(PLATFORM_KEY),
        PATCH,
    )
    .await;
    assert_eq!(bad_id.status(), StatusCode::BAD_REQUEST);
    let bad_delete = send(
        admin,
        Method::DELETE,
        "/v1/admin/models/not-a-uuid",
        Some(PLATFORM_KEY),
        "",
    )
    .await;
    assert_eq!(bad_delete.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(bad_delete)
            .await
            .get("detail")
            .and_then(serde_json::Value::as_str),
        Some("id must be a canonical UUIDv7")
    );
}

/// A platform key whose provider and `base_url` do not belong together is
/// refused as a provider mistake before any datastore is asked.
async fn refused_as_an_unusable_pairing(admin: &axum::Router, body: &str) {
    let refused = send(
        admin,
        Method::PUT,
        "/v1/admin/platform-keys",
        Some(PLATFORM_KEY),
        body,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code(refused).await, Some("UZ-PROVIDER-005"));
}

mod library;

#[tokio::test]
async fn model_edits_and_deletion_report_datastore_unavailability() {
    let admin = fleet(Scope::ModelAdmin);
    for method in [Method::PATCH, Method::DELETE] {
        let response = send(
            &admin,
            method,
            &format!("/v1/admin/models/{MODEL_ID}"),
            Some(PLATFORM_KEY),
            PATCH,
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(code(response).await, Some("UZ-INTERNAL-001"));
    }
}

async fn code(response: axum::response::Response) -> Option<&'static str> {
    match json_body(response)
        .await
        .get("error_code")
        .and_then(serde_json::Value::as_str)
    {
        Some("UZ-INTERNAL-001") => Some("UZ-INTERNAL-001"),
        Some("UZ-REQ-001") => Some("UZ-REQ-001"),
        Some("UZ-REQ-002") => Some("UZ-REQ-002"),
        Some("UZ-PROVIDER-005") => Some("UZ-PROVIDER-005"),
        _other => None,
    }
}
