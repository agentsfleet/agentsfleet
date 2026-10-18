//! Provider resolution through its real selection, default and vault rows.

use afd_crypto::entropy::Entropy;
use std::sync::Arc;

use afd_billing::Posture;
use afd_credential::provider::{Added, Providers};
use afd_credential::secrets::Registry;

use super::{Fixture, NOW};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_self_managed_selection_resolves_from_the_tenants_primary_workspace() {
    let fixture = Fixture::create().await;
    let secret = "self-managed-provider";
    fixture
        .seed_with_shape(
            secret,
            br#"{"provider":"openai","api_key":"sk-self-managed"}"#,
            Some("openai"),
            Some(true),
        )
        .await;
    // The ceiling is the catalogue's, read live: a negative row is one the
    // schema permits, and resolution clamps it to the unknown/auto sentinel.
    let model = format!("gpt-fixture-{}", afd_db::test_util::mint_id());
    fixture.seed_catalogue("openai", &model, -1).await;
    let store = Providers::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    );
    let Added::Stored(entry) = store
        .add_entry(&fixture.tenant, &model, secret, NOW)
        .await
        .expect("an entry over a stored credential is an outcome")
    else {
        panic!("the credential is stored, so the entry stores");
    };
    fixture.point_selection_at(&fixture.tenant, &entry.id).await;

    let resolved = store
        .resolve(&fixture.tenant)
        .await
        .expect("the selection resolves through the tenant workspace");

    assert_eq!(resolved.posture, Posture::SelfManaged);
    assert_eq!(&*resolved.provider, "openai");
    assert_eq!(&*resolved.model, model.as_str());
    assert_eq!(
        resolved.context_cap_tokens, 0,
        "a negative catalogue cap clamps"
    );
    assert_eq!(resolved.api_key().expose(), "sk-self-managed");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_with_no_selection_resolves_the_platform_default() {
    // The arm every tenant that never chose a provider takes. The default row
    // names the workspace its key lives in; the tenant's own vault is not read.
    let fixture = Fixture::create().await;
    let provider = format!("platform-fixture-{}", afd_db::test_util::mint_id());
    fixture
        .seed(
            &provider,
            br#"{"provider":"anthropic","api_key":"sk-platform"}"#,
        )
        .await;
    let model = format!("model-fixture-{}", afd_db::test_util::mint_id());
    fixture.seed_catalogue(&provider, &model, 64_000).await;
    let held = fixture
        .seed_platform_default(&provider, &model, 64_000)
        .await;

    let resolved = Providers::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
    .resolve(&fixture.tenant)
    .await
    .expect("the platform default resolves");

    assert_eq!(resolved.posture, Posture::Platform);
    assert_eq!(&*resolved.provider, provider.as_str());
    assert_eq!(resolved.api_key().expose(), "sk-platform");
    drop(held);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_undeclared_connector_is_never_injected() {
    // The workspace holds TWO connector handles; the fleet declares one. The
    // lease's declared set must carry exactly the one, in both halves — the
    // static map and the mintable list — and nothing of the other. The
    // positive case beside this proves what declared names resolve to; this is
    // the negative it was missing: an undeclared name is not merely absent
    // from the request, it is absent from the answer.
    let fixture = Fixture::create().await;
    fixture
        .seed(
            "github",
            br#"{"integration":"github","installation_id":"42","app_id":"7"}"#,
        )
        .await;
    fixture
        .seed(
            "slack",
            br#"{"integration":"slack","bot_token":"xoxb-never-injected","team_id":"TEAMFIXTURE"}"#,
        )
        .await;
    let connectors = Registry::default();
    let declared = fixture
        .vault
        .declared(&fixture.workspace, &["github"], &connectors)
        .await
        .expect("the one declared credential resolves");
    assert!(
        declared.secrets_map().get("slack").is_none(),
        "the undeclared Slack handle is not in the static map"
    );
    let mintable: Vec<&str> = declared
        .mintable()
        .iter()
        .map(|entry| &*entry.name)
        .collect();
    assert_eq!(
        mintable,
        vec!["github"],
        "only the declared connector is mintable"
    );
    let rendered = format!("{:?}", declared.secrets_map());
    assert!(
        !rendered.contains("xoxb-never-injected"),
        "the undeclared token reaches nothing the lease is built from"
    );
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
