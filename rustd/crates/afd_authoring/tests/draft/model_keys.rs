//! Key opening rechecks the owner and admitted snapshot under one transaction.

use super::fixture::Fixture;
use afd_core::error_code;
use afd_credential::provider::{Providers, registered_model};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_vault::{SecretBody, SecretName, Vault};
use sqlx::Connection as _;
use std::sync::Arc;

fn key() -> Kek {
    Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture encryption key")
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn saved_workspace_reference_resolves_only_its_owners_key_and_model_settings() {
    let f = Fixture::create().await;
    let providers = Providers::new(f.db.clone(), Arc::new(key()), Entropy::new());
    let resolved = providers
        .resolve_reference(&f.tenant, f.model.as_str())
        .await
        .expect("saved workspace model and owner credential resolve together");
    assert_eq!(resolved.posture, afd_billing::Posture::SelfManaged);
    assert_eq!(resolved.api_key().expose(), "fixture-authoring-key");
    assert_eq!(resolved.provider.as_ref(), "openai");
    assert_eq!(
        resolved.model.as_ref(),
        format!("workspace-model-{}", f.catalogue[0])
    );
    assert_eq!(resolved.context_cap_tokens, 128_000);
    let inference = resolved.inference.expect("saved inference settings");
    assert_eq!(inference.api, afd_wire::inference::InferenceApi::Responses);
    assert_eq!(inference.base_url, "https://models.example.com/v1");
    assert_eq!(inference.max_output_tokens, 8192);
    let refused = providers
        .resolve_reference(&f.member_tenant, f.model.as_str())
        .await
        .expect_err("another owner cannot open the saved workspace credential");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(refused.is_config_permanent());
    assert!(!refused.to_string().contains("fixture-authoring-key"));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn key_opening_refuses_a_foreign_owner_and_changed_admitted_model() {
    let f = Fixture::create().await;
    let providers = Providers::new(f.db.clone(), Arc::new(key()), Entropy::new());
    let mut connection = f.connection().await;
    let mut tx = connection.begin().await.expect("dispatch transaction");
    let mut pinned = registered_model(&mut tx, &f.tenant, f.model.as_str())
        .await
        .expect("metadata read")
        .expect("configured owner model");
    let opened = providers
        .registered_key(&mut tx, &f.tenant, &pinned)
        .await
        .expect("matching generation")
        .expect("owner credential");
    assert_eq!(opened.expose(), "fixture-authoring-key");
    assert!(
        providers
            .registered_key(&mut tx, &f.member_tenant, &pinned)
            .await
            .expect("foreign owner refusal")
            .is_none()
    );
    pinned.model = "changed-after-admission".into();
    assert!(
        providers
            .registered_key(&mut tx, &f.tenant, &pinned)
            .await
            .expect("changed snapshot refusal")
            .is_none()
    );
    tx.rollback().await.expect("release credential lock");
    drop(connection);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn key_opening_refuses_an_empty_sealed_key_even_if_its_projection_claims_presence() {
    let f = Fixture::create().await;
    let raw =
        serde_json::value::RawValue::from_string(r#"{"provider":"openai","api_key":""}"#.into())
            .expect("empty credential body");
    Vault::new(f.db.clone(), Arc::new(key()), Entropy::new())
        .replace(
            &f.workspace,
            &SecretName::parse("authoring-model").expect("fixture name"),
            &SecretBody::parse(&raw).expect("credential shape"),
        )
        .await
        .expect("seal empty key");
    sqlx::query("UPDATE vault.secrets SET meta_has_key=true WHERE workspace_id=$1::uuid AND key_name='authoring-model'")
        .bind(f.workspace.as_str()).execute(&mut *f.connection().await).await
        .expect("simulate inconsistent presence metadata");
    let providers = Providers::new(f.db.clone(), Arc::new(key()), Entropy::new());
    let mut connection = f.connection().await;
    let mut tx = connection.begin().await.expect("dispatch transaction");
    let pinned = registered_model(&mut tx, &f.tenant, f.model.as_str())
        .await
        .expect("metadata read")
        .expect("presence projection accepted");
    let error = providers
        .registered_key(&mut tx, &f.tenant, &pinned)
        .await
        .expect_err("the sealed credential is authoritative");
    assert_eq!(error.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(error.is_config_permanent());
    tx.rollback().await.expect("release credential lock");
    drop(connection);
    f.cleanup().await;
}
