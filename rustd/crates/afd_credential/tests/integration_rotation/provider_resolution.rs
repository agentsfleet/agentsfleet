//! Self-managed provider resolution through its real selection and vault rows.

use std::sync::Arc;

use afd_billing::Posture;
use afd_credential::provider::Providers;
use afd_credential::secrets::Registry;

use super::Fixture;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_self_managed_selection_resolves_from_the_tenants_primary_workspace() {
    let fixture = Fixture::create().await;
    let secret = "self-managed-provider";
    fixture
        .seed(
            secret,
            br#"{"provider":"openai","api_key":"sk-self-managed"}"#,
        )
        .await;
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query(
        "INSERT INTO core.tenant_model_selection \
           (tenant_id, mode, provider, model, context_cap_tokens, secret_ref, \
            created_at, updated_at) \
         VALUES ($1::uuid, $3, 'selection-provider-is-provenance', \
                 'gpt-fixture', -1, $2, 1, 1)",
    )
    .bind(fixture.tenant.as_str())
    .bind(secret)
    // Bound from the constant the resolver parses against, never re-spelled.
    // This row said 'byok', which this codebase has never written: the parse
    // refuses an unknown posture rather than guessing `platform`, so the
    // fixture failed the moment it was first run.
    .bind(afd_billing::sql::posture::SELF_MANAGED)
    .execute(&mut *connection)
    .await
    .expect("the self-managed selection seeds");
    drop(connection);

    let resolved = Providers::new(fixture.database.clone(), Arc::clone(&fixture.kek))
        .resolve(&fixture.tenant)
        .await
        .expect("the selection resolves through the tenant workspace");

    assert_eq!(resolved.posture, Posture::SelfManaged);
    assert_eq!(&*resolved.provider, "openai");
    assert_eq!(&*resolved.model, "gpt-fixture");
    assert_eq!(
        resolved.context_cap_tokens, 0,
        "a negative stored cap clamps"
    );
    assert_eq!(resolved.api_key().expose(), "sk-self-managed");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn declared_credentials_batch_static_and_mintable_rows_without_leaking_handles() {
    let fixture = Fixture::create().await;
    fixture.seed("static", br#"{"token":"stored-value"}"#).await;
    fixture
        .seed(
            "github",
            br#"{"integration":"github","installation_id":"42","app_id":"7"}"#,
        )
        .await;
    let connectors = Registry::default();

    let declared = fixture
        .vault
        .declared(&fixture.workspace, &["static", "github"], &connectors)
        .await
        .expect("both declared credentials resolve in one batch");
    assert_eq!(
        declared
            .secrets_map()
            .get("static")
            .and_then(|value| value.get("token"))
            .and_then(serde_json::Value::as_str),
        Some("stored-value")
    );
    assert_eq!(declared.mintable().len(), 1);
    assert_eq!(
        &*declared
            .mintable()
            .first()
            .expect("one mintable entry was counted")
            .name,
        "github"
    );

    let missing = fixture
        .vault
        .declared(&fixture.workspace, &["absent"], &connectors)
        .await
        .expect_err("a declared but absent credential ends the lease");
    assert!(missing.is_credential_missing());

    fixture.seed("invalid", br#""not-an-object""#).await;
    let invalid = fixture
        .vault
        .declared(&fixture.workspace, &["invalid"], &connectors)
        .await
        .expect_err("a static credential must be a JSON object");
    assert_eq!(invalid.code(), afd_core::error_code::VAULT_DATA_INVALID);
    fixture.cleanup().await;
}
