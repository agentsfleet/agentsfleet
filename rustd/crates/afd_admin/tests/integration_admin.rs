//! Platform administration proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_admin::{
    CreateModel, DeleteModel, ModelInput, ModelRates, Models, PlatformKeyInput, PlatformKeys,
    SetPlatformKey,
};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;

const NOW: UnixMillis = UnixMillis::from_millis(1_725_000_000_000);

const SEED: &str = r"
WITH tenant AS (
    INSERT INTO core.tenants (id, name, created_at, updated_at)
    VALUES ($1::uuid, 'Admin test', 1, 1)
)
INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
VALUES ($2::uuid, $1::uuid, 'primary', 'test', 2)
";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn model_and_platform_key_mutations_are_atomic() {
    let fixtures = Fixtures::create().await;
    fixtures.seed().await;
    let (entropy, _control) = Entropy::new_mocked();
    let models = Models::new(fixtures.database.clone(), entropy);
    let keys = PlatformKeys::new(fixtures.database.clone());
    let provider = fixtures.provider();
    let model_id = fixtures.model_id();

    let anthropic = create(&models, &provider, &model_id).await;
    assert_model_created_and_duplicate(&models, &provider, &model_id, &anthropic).await;
    assert_missing_key_dependencies(&keys, &fixtures, &provider, &model_id).await;
    assert_active_key_guards_model(&models, &keys, &fixtures, &provider, &model_id, &anthropic)
        .await;
    fixtures.cleanup().await;
}

async fn assert_model_created_and_duplicate(
    models: &Models,
    provider: &str,
    model_id: &str,
    anthropic: &afd_admin::Model,
) {
    assert!(
        models
            .list()
            .await
            .expect("models list")
            .iter()
            .any(|model| model.id() == anthropic.id())
    );
    assert_eq!(
        models
            .create(
                &input(provider, model_id),
                UnixMillis::from_millis(NOW.as_millis() + 1),
            )
            .await
            .expect("duplicate is a typed outcome"),
        CreateModel::Duplicate
    );
}

async fn assert_missing_key_dependencies(
    keys: &PlatformKeys,
    fixtures: &Fixtures,
    provider: &str,
    model_id: &str,
) {
    let missing_workspace = PlatformKeyInput::new(
        provider.to_owned(),
        id(fixtures.missing_workspace.as_str()),
        model_id.to_owned(),
        None,
    );
    assert_eq!(
        keys.set(&missing_workspace, NOW)
            .await
            .expect("missing workspace is a typed outcome"),
        SetPlatformKey::WorkspaceNotFound
    );
    assert!(
        keys.list()
            .await
            .expect("keys list")
            .iter()
            .all(|key| key.provider() != provider)
    );

    let missing_model = PlatformKeyInput::new(
        provider.to_owned(),
        id(fixtures.workspace.as_str()),
        "missing".to_owned(),
        None,
    );
    assert_eq!(
        keys.set(&missing_model, NOW)
            .await
            .expect("missing model is a typed outcome"),
        SetPlatformKey::ModelNotFound
    );
    assert!(
        keys.list()
            .await
            .expect("keys list")
            .iter()
            .all(|key| key.provider() != provider)
    );
}

async fn assert_active_key_guards_model(
    models: &Models,
    keys: &PlatformKeys,
    fixtures: &Fixtures,
    provider: &str,
    model_id: &str,
    anthropic: &afd_admin::Model,
) {
    let active = PlatformKeyInput::new(
        provider.to_owned(),
        id(fixtures.workspace.as_str()),
        model_id.to_owned(),
        None,
    );
    let SetPlatformKey::Set(key) = keys.set(&active, NOW).await.expect("default activates") else {
        panic!("a valid default must activate");
    };
    assert!(key.is_active());
    assert_eq!(key.model(), Some(model_id));
    assert_eq!(
        models
            .delete(anthropic.id(), NOW)
            .await
            .expect("reference check reads"),
        DeleteModel::InUse
    );

    assert!(
        keys.deactivate(provider, NOW)
            .await
            .expect("default deactivates")
    );
    assert_eq!(
        models
            .delete(anthropic.id(), NOW)
            .await
            .expect("unreferenced model deletes"),
        DeleteModel::Deleted
    );
}

async fn create(models: &Models, provider: &str, model_id: &str) -> afd_admin::Model {
    let CreateModel::Created(model) = models
        .create(&input(provider, model_id), NOW)
        .await
        .expect("model creates")
    else {
        panic!("the fixture model must be new");
    };
    model
}

fn input(provider: &str, model_id: &str) -> ModelInput {
    ModelInput::new(
        provider.to_owned(),
        model_id.to_owned(),
        ModelRates::new(200_000, 5, 1, 25),
    )
}

fn id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).expect("the fixture id is UUIDv7")
}

struct Fixtures {
    lane: TestDatabase,
    database: Db,
    tenant: Uuid7,
    workspace: Uuid7,
    missing_workspace: Uuid7,
    suffix: String,
}

impl Fixtures {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let tenant = id(&afd_db::test_util::mint_id());
        let workspace = id(&afd_db::test_util::mint_id());
        let missing_workspace = id(&afd_db::test_util::mint_id());
        let suffix = tenant.as_str().replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            lane,
            tenant,
            workspace,
            missing_workspace,
            suffix,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(SEED)
            .bind(self.tenant.as_str())
            .bind(self.workspace.as_str())
            .execute(&mut *connection)
            .await
            .expect("the admin fixture seeds");
    }

    fn provider(&self) -> String {
        format!("anthropic-{}", self.suffix)
    }

    fn model_id(&self) -> String {
        format!("claude-opus-5-{}", self.suffix)
    }

    async fn cleanup(self) {
        drop(self.database);
        self.lane.cleanup().await;
    }
}
