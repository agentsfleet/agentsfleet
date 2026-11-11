//! Model references retain the exact workspace that holds their credential.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_credential::provider::{Activation, Added, Entry, Providers, registered_model};
use afd_credential::vault::KeyRef;
use afd_db::test_util::{ModelSeed, WorkspaceSeed, mint_id};
use afd_vault::{Deleted, Directory, SecretBody, SecretName, Vault};

use super::registry::providers;
use super::{Fixture, NOW};

const NAME: &str = "shared-provider-name";
const MAIN_KEY: &str = "main-credential";
const STAGING_KEY: &str = "staging-credential";

async fn another_workspace(fixture: &Fixture) -> Uuid7 {
    let workspace = mint_id();
    WorkspaceSeed::new(&workspace, &fixture.tenant)
        .insert(
            &mut *fixture
                .database
                .acquire()
                .await
                .expect("workspace connection"),
        )
        .await
        .expect("the same account can hold another workspace");
    workspace
}

async fn save_key(fixture: &Fixture, workspace: &Uuid7, key: &str) {
    let raw = serde_json::value::RawValue::from_string(
        serde_json::json!({"provider": "openai", "api_key": key}).to_string(),
    )
    .expect("fixture credential JSON");
    Vault::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
    .create(
        workspace,
        &SecretName::parse(NAME).expect("fixture name"),
        &SecretBody::parse(&raw).expect("fixture provider credential"),
        NOW,
    )
    .await
    .expect("a key stores in its explicit workspace");
}

async fn register(store: &Providers, fixture: &Fixture, workspace: &Uuid7, model: &str) -> Entry {
    let result = store
        .add_entry(
            &fixture.tenant,
            model,
            KeyRef {
                workspace_id: workspace,
                name: NAME,
            },
            NOW,
        )
        .await
        .expect("registration answers");
    let Added::Stored(entry) = result else {
        panic!("an independent model stores: {result:?}");
    };
    assert_eq!(entry.workspace_id.as_ref(), Some(workspace));
    entry
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn same_named_keys_remain_independent_when_switching_the_active_model() {
    let fixture = Fixture::create().await;
    let staging = another_workspace(&fixture).await;
    save_key(&fixture, &fixture.workspace, MAIN_KEY).await;
    save_key(&fixture, &staging, STAGING_KEY).await;
    let model = format!("workspace-model-{}", mint_id());
    ModelSeed::new(&mint_id(), "openai", &model, 64_000)
        .verified_at(NOW)
        .inference(r#"{"api":"responses","base_url":"https://api.openai.com/v1","max_output_tokens":8192}"#)
        .insert(&mut *fixture.database.acquire().await.expect("catalogue connection"))
        .await.expect("a routable priced model seeds");
    let store = providers(&fixture);
    let main = register(&store, &fixture, &fixture.workspace, &model).await;
    let staged = register(&store, &fixture, &staging, &model).await;
    assert_ne!(
        main.id, staged.id,
        "workspace is part of the credential's identity"
    );

    let page = store
        .registry_page(&fixture.tenant, 20, None)
        .await
        .expect("registry reads");
    assert_eq!(page.rows.len(), 2);
    for row in &page.rows {
        assert!(row.credential.as_ref().is_some_and(|key| key.has_key));
        assert!(
            row.rate.is_some(),
            "both exact references receive catalogue metadata"
        );
    }

    for (workspace, expected_id, expected_key) in [
        (&staging, &staged.id, STAGING_KEY),
        (&fixture.workspace, &main.id, MAIN_KEY),
    ] {
        let result = store
            .activate(
                &fixture.tenant,
                KeyRef {
                    workspace_id: workspace,
                    name: NAME,
                },
                Some(&model),
                NOW,
            )
            .await
            .expect("activation answers");
        let Activation::Applied(own) = result else {
            panic!("the model activates: {result:?}");
        };
        assert_eq!(&own.tenant_model_id, expected_id);
        assert_eq!(own.workspace_id.as_ref(), Some(workspace));
        let resolved = store
            .resolve(&fixture.tenant)
            .await
            .expect("the selected credential resolves");
        assert_eq!(resolved.api_key().expose(), expected_key);
    }

    // An existing fleet's pinned model stays on Staging after Main becomes
    // the account default. The registered read and key open use the same pair.
    let mut connection = fixture
        .database
        .acquire()
        .await
        .expect("runtime connection");
    let pinned = registered_model(&mut connection, &fixture.tenant, staged.id.as_str())
        .await
        .expect("registered metadata reads")
        .expect("the staged model remains available");
    let key = store
        .registered_key(&mut connection, &fixture.tenant, &pinned)
        .await
        .expect("the pinned key opens")
        .expect("the generation is unchanged");
    assert_eq!(key.expose(), STAGING_KEY);
    drop(connection);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn deletion_guards_only_the_exact_workspace_and_secret_name() {
    let fixture = Fixture::create().await;
    let staging = another_workspace(&fixture).await;
    save_key(&fixture, &fixture.workspace, MAIN_KEY).await;
    save_key(&fixture, &staging, STAGING_KEY).await;
    let store = providers(&fixture);
    register(&store, &fixture, &staging, "staged-model").await;
    let directory = Directory::new(fixture.database.clone());
    let name = SecretName::parse(NAME).expect("fixture name");
    assert_eq!(
        directory
            .delete(&fixture.workspace, &name)
            .await
            .expect("Main's key is unused"),
        Deleted::Removed
    );
    let refused = directory
        .delete(&staging, &name)
        .await
        .expect_err("Staging's key remains referenced");
    assert_eq!(
        refused.code(),
        af_core::error_code::SECRET_REFERENCED_BY_MODEL_ENTRIES
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn another_accounts_workspace_cannot_supply_a_model_credential() {
    let owner = Fixture::create().await;
    let other = Fixture::create().await;
    save_key(&owner, &owner.workspace, MAIN_KEY).await;
    let store = providers(&owner);
    let foreign = owner.key(NAME);
    assert_eq!(
        store
            .add_entry(&other.tenant, "foreign-model", foreign, NOW)
            .await
            .expect("registration answers"),
        Added::CredentialMissing
    );
    assert_eq!(
        store
            .activate(&other.tenant, foreign, Some("foreign-model"), NOW)
            .await
            .expect("activation answers"),
        Activation::CredentialMissing
    );
    assert!(
        store
            .registry_page(&other.tenant, 20, None)
            .await
            .expect("registry reads")
            .rows
            .is_empty()
    );
    assert!(
        store
            .selection(&other.tenant)
            .await
            .expect("selection reads")
            .is_none()
    );
    other.cleanup().await;
    owner.cleanup().await;
}
