//! Model generations detect key rotation without relying on timestamp granularity.

use super::fixture::{Fixture, now};
use afd_credential::provider::registered_model;
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_vault::{SecretBody, SecretName, Vault};
use std::sync::Arc;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rotating_with_the_same_timestamp_changes_resolution_generation() {
    let f = Fixture::create().await;
    let mut connection = f.connection().await;
    let before = registered_model(&mut connection, &f.tenant, f.model.as_str())
        .await
        .expect("resolve metadata")
        .expect("registered owner model");
    assert!(
        registered_model(&mut connection, &f.member_tenant, f.model.as_str())
            .await
            .expect("foreign metadata read")
            .is_none(),
        "a creator's own account never supplies the owner's reference"
    );
    let timestamp: jiff_sqlx::Timestamp = sqlx::query_scalar("SELECT updated_at FROM vault.secrets WHERE workspace_id=$1::uuid AND key_name='authoring-model'")
        .bind(f.workspace.as_str()).fetch_one(&mut *connection).await.expect("original timestamp");
    drop(connection);
    rotate(&f).await;
    let after = registered_model(&mut *f.connection().await, &f.tenant, f.model.as_str())
        .await
        .expect("resolve rotated metadata")
        .expect("same registered reference");
    assert_eq!(before.reference, after.reference);
    assert_eq!(before.model, after.model);
    assert_ne!(before.resolution_version, after.resolution_version);
    assert!(timestamp.to_jiff() <= now());
    f.cleanup().await;
}

/// Rotates the key in place; its sealed bytes change even when the database
/// stamps the same millisecond.
async fn rotate(f: &Fixture) {
    let raw = serde_json::value::RawValue::from_string(
        r#"{"provider":"openai","api_key":"rotated-fixture-key"}"#.to_owned(),
    )
    .expect("fixture body JSON");
    let body = SecretBody::parse(&raw).expect("credential body");
    let kek = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture encryption key");
    Vault::new(f.db.clone(), Arc::new(kek), Entropy::new())
        .replace(
            &f.workspace,
            &SecretName::parse("authoring-model").expect("fixture name"),
            &body,
        )
        .await
        .expect("rotate the fixture key");
}
