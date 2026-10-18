//! Refresh-token rotation against a locked, encrypted vault row, and the
//! platform's own credentials loading row by row.

use afd_core::error_code;
use afd_credential::credential::platform::Platform;
use afd_credential::vault::rotate::Rotated;
use serde_json::Value;

use super::{Fixture, KEY_NAME};

/// The handle stored under `name`, opened and parsed.
async fn stored_handle(fixture: &Fixture, name: &str) -> Value {
    let opened = fixture
        .vault
        .open(fixture.key(name))
        .await
        .expect("the envelope opens")
        .expect("the row exists");
    serde_json::from_slice(opened.expose()).expect("the handle is JSON")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn refresh_rotation_persists_only_the_handle_that_was_redeemed() {
    let fixture = Fixture::create().await;
    fixture
        .seed(KEY_NAME, br#"{"refresh_token":"old","account":"kept"}"#)
        .await;
    let key = fixture.key(KEY_NAME);

    assert_eq!(
        fixture
            .vault
            .rotate_refresh_token(key, "old", "replacement")
            .await
            .expect("the matching handle rotates"),
        Rotated::Persisted
    );
    let body = stored_handle(&fixture, KEY_NAME).await;
    assert_eq!(
        body.get("refresh_token").and_then(Value::as_str),
        Some("replacement")
    );
    assert_eq!(body.get("account").and_then(Value::as_str), Some("kept"));

    assert_eq!(
        fixture
            .vault
            .rotate_refresh_token(key, "old", "must-not-win")
            .await
            .expect("a stale exchange is a typed outcome"),
        Rotated::SkippedStale
    );
    assert_eq!(
        fixture
            .vault
            .rotate_refresh_token(fixture.key("missing"), "old", "new")
            .await
            .expect("a deleted handle is a typed outcome"),
        Rotated::SkippedStale
    );

    fixture.seed("malformed", b"[]").await;
    let invalid = fixture
        .vault
        .rotate_refresh_token(fixture.key("malformed"), "old", "new")
        .await
        .expect_err("a stored handle must be a JSON object");
    assert_eq!(invalid.code(), error_code::VAULT_DATA_INVALID);

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_credentials_load_independently_and_degrade_bad_rows() {
    let fixture = Fixture::create().await;
    fixture
        .seed("github-app", br#"{"app_id":"42","private_key_pem":"pem"}"#)
        .await;
    fixture
        .seed(
            "zoho-app",
            br#"{"client_id":"zoho-client","client_secret":"secret"}"#,
        )
        .await;
    fixture.seed("jira-app", b"not-json").await;
    fixture.seed("linear-app", b"{}").await;

    let platform = Platform::load(&fixture.vault, &fixture.workspace).await;

    assert_eq!(platform.github().map(|app| app.app_id), Some(42));
    assert_eq!(
        platform.oauth("zoho").map(|app| app.client_id.as_str()),
        Some("zoho-client")
    );
    assert!(platform.oauth("jira").is_none());
    assert!(platform.oauth("linear").is_none());

    fixture.cleanup().await;
}
