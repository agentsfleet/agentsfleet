//! Platform administration proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_admin::{
    CreateModel, DeleteModel, Model, ModelInput, ModelUpdate, Models, PlatformKeyInput,
    PlatformKeys, SetPlatformKey,
};
use afd_core::catalogue::{Metadata, Price};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TenantSeed, TestDatabase, WorkspaceSeed, mint_id, mint_uuid7};
use afd_db::{Db, Migrator};
use jiff::Timestamp;

/// The instant model identifiers are minted at; rows are stamped by `now()`.
const NOW: Timestamp = Timestamp::constant(1_725_000_000, 0);

#[path = "admin_model/proofs.rs"]
mod model_proofs;

/// Synthetic prices and API evidence for the administrator's fixture model.
fn details(raised: bool) -> ModelUpdate {
    ModelUpdate {
        metadata: Metadata {
            display_name: "Administrator fixture".to_owned(),
            context_cap_tokens: if raised { 400_000 } else { 200_000 },
            context_source_url: "https://example.test/models/fixture".to_owned(),
            context_verified_at: NOW,
            advanced: raised,
        },
        prices: vec![Price {
            effective_from: NOW,
            effective_until: None,
            prompt_tokens_above: 0,
            input_nanos_per_mtok: if raised { 6 } else { 5 },
            cached_input_nanos_per_mtok: if raised { 2 } else { 1 },
            cache_write_nanos_per_mtok: Some(187_500_000),
            cache_write_5m_nanos_per_mtok: Some(125_000_000),
            cache_write_1h_nanos_per_mtok: Some(200_000_000),
            output_nanos_per_mtok: if raised { 30 } else { 25 },
            source_url: "https://example.test/pricing".to_owned(),
            verified_at: NOW,
            conditions: "Synthetic administrator fixture; no provider request.".to_owned(),
        }],
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_file_list_update_and_refuse_a_duplicate_model() {
    let lane = Lane::create().await;
    let model = lane.file_model().await;

    let listed = lane.models.list().await.expect("models list");
    assert!(
        listed.contains(&model),
        "the list reads back what create wrote"
    );
    assert_eq!(
        lane.models
            .create(lane.input(), NOW)
            .await
            .expect("a typed outcome"),
        CreateModel::Duplicate
    );

    assert!(
        lane.models
            .update(&model.id, details(true))
            .await
            .expect("updates")
    );
    let updated = lane.models.list().await.expect("models list");
    let row = updated
        .iter()
        .find(|listed| listed.id == model.id)
        .expect("the model is still filed");
    assert_eq!(row.details, details(true));
    assert!(
        !lane
            .models
            .update(&mint_uuid7(), details(true))
            .await
            .expect("a typed outcome"),
        "an unknown id updates nothing"
    );
    assert_eq!(
        lane.models
            .delete(&mint_uuid7())
            .await
            .expect("a typed outcome"),
        DeleteModel::NotFound
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_refuse_a_default_whose_workspace_or_model_has_no_row() {
    let lane = Lane::create().await;
    lane.file_model().await;

    let missing_workspace = PlatformKeyInput {
        source_workspace_id: mint_uuid7(),
        ..lane.key_input()
    };
    assert_eq!(
        lane.keys
            .set(missing_workspace)
            .await
            .expect("a typed outcome"),
        SetPlatformKey::WorkspaceNotFound
    );
    let missing_model = PlatformKeyInput {
        model: "missing".to_owned(),
        ..lane.key_input()
    };
    assert_eq!(
        lane.keys.set(missing_model).await.expect("a typed outcome"),
        SetPlatformKey::ModelNotFound
    );
    let listed = lane.keys.list().await.expect("keys list");
    assert!(
        listed.iter().all(|key| key.provider != lane.provider),
        "a refused set leaves no default behind"
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_guard_a_model_while_an_active_default_points_at_it() {
    let lane = Lane::create().await;
    let model = lane.file_model().await;

    let SetPlatformKey::Set(key) = lane.keys.set(lane.key_input()).await.expect("activates") else {
        unreachable!("a catalogued model and a live workspace activate");
    };
    assert!(key.active);
    assert_eq!(key.model.as_deref(), Some(lane.model_name.as_str()));
    let listed = lane
        .keys
        .list()
        .await
        .expect("keys list")
        .into_iter()
        .find(|listed| listed.provider == lane.provider)
        .expect("the activated default lists");
    assert_eq!(listed, key, "the list reads back what the upsert returned");
    assert_eq!(listed.source_workspace_id, lane.workspace);

    assert_eq!(
        lane.models
            .delete(&model.id)
            .await
            .expect("a typed outcome"),
        DeleteModel::InUse
    );
    assert!(
        lane.keys
            .deactivate(&lane.provider)
            .await
            .expect("deactivates")
    );
    assert!(
        !lane
            .keys
            .deactivate(&mint_id())
            .await
            .expect("a typed outcome"),
        "an unknown provider deactivates nothing"
    );
    assert_eq!(
        lane.models
            .delete(&model.id)
            .await
            .expect("a typed outcome"),
        DeleteModel::Deleted
    );
    lane.cleanup().await;
}

/// The lane's database, the two stores, and one seeded workspace.
struct Lane {
    owner: TestDatabase,
    database: Db,
    models: Models,
    keys: PlatformKeys,
    workspace: Uuid7,
    provider: String,
    model_name: String,
}

impl Lane {
    async fn create() -> Self {
        let owner = TestDatabase::create().await;
        let migrator = owner.open(DbRole::Migrator, &[]).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema migrates");
        migrator.close().await;
        let database: Db = owner.open(DbRole::Api, &[]).await;
        let tenant = mint_uuid7();
        let workspace = mint_uuid7();
        let mut connection = database.acquire().await.expect("an API connection");
        TenantSeed::new(tenant.as_str())
            .insert(&mut *connection)
            .await
            .expect("seeding the tenant");
        WorkspaceSeed::new(workspace.as_str(), tenant.as_str())
            .insert(&mut *connection)
            .await
            .expect("seeding the workspace");
        let suffix = tenant.as_str().replace('-', "");
        Self {
            models: Models::new(database.clone(), Entropy::new()),
            keys: PlatformKeys::new(database.clone()),
            database,
            owner,
            workspace,
            provider: format!("anthropic-{suffix}"),
            model_name: format!("claude-opus-5-{suffix}"),
        }
    }

    async fn cleanup(self) {
        self.database.close().await;
        self.owner.cleanup().await;
    }

    fn input(&self) -> ModelInput {
        ModelInput {
            provider: self.provider.clone(),
            model_name: self.model_name.clone(),
            details: details(false),
        }
    }

    fn key_input(&self) -> PlatformKeyInput {
        PlatformKeyInput {
            provider: self.provider.clone(),
            source_workspace_id: self.workspace.clone(),
            model: self.model_name.clone(),
            base_url: None,
        }
    }

    async fn file_model(&self) -> Model {
        let CreateModel::Created(model) = self
            .models
            .create(self.input(), NOW)
            .await
            .expect("creates")
        else {
            unreachable!("the fixture model is new: its name carries a minted suffix");
        };
        assert_eq!(model.details, details(false));
        model
    }
}
