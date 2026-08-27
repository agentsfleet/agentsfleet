//! Platform administration proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_admin::{
    CreateModel, DeleteModel, ModelInput, ModelRates, Models, PlatformKeyInput, PlatformKeys,
    SetPlatformKey,
};
use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::{Db, Migrator};
use sqlx::AssertSqlSafe;

const LANE_KNOB: &str = "TEST_DATABASE_URL";
const WORKSPACE: &str = "01950000-0000-7000-8000-000000000002";
const MISSING_WORKSPACE: &str = "01950000-0000-7000-8000-000000000099";
const NOW: UnixMillis = UnixMillis::from_millis(1_725_000_000_000);

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

const SEED: &str = r"
WITH tenant AS (
    INSERT INTO core.tenants (id, name, created_at, updated_at)
    VALUES ('01950000-0000-7000-8000-000000000001', 'Admin test', 1, 1)
)
INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
VALUES ('01950000-0000-7000-8000-000000000002', '01950000-0000-7000-8000-000000000001', 'primary', 'test', 2)
";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn model_and_platform_key_mutations_are_atomic() {
    let fixtures = Fixtures::create().await;
    fixtures.seed().await;
    let (entropy, _control) = Entropy::new_mocked();
    let models = Models::new(fixtures.database.clone(), entropy);
    let keys = PlatformKeys::new(fixtures.database.clone());

    let anthropic = create(&models, "anthropic", "claude-opus-5").await;
    assert_eq!(models.list().await.expect("models list").len(), 1);
    let revision = fixtures.revision().await;
    assert_eq!(
        models
            .create(
                &input("anthropic", "claude-opus-5"),
                UnixMillis::from_millis(NOW.as_millis() + 1),
            )
            .await
            .expect("duplicate is a typed outcome"),
        CreateModel::Duplicate
    );
    assert_eq!(fixtures.revision().await, revision);

    let missing_workspace = PlatformKeyInput::new(
        "anthropic".to_owned(),
        id(MISSING_WORKSPACE),
        "claude-opus-5".to_owned(),
        None,
    );
    assert_eq!(
        keys.set(&missing_workspace, NOW)
            .await
            .expect("missing workspace is a typed outcome"),
        SetPlatformKey::WorkspaceNotFound
    );
    assert!(keys.list().await.expect("keys list").is_empty());

    let missing_model = PlatformKeyInput::new(
        "anthropic".to_owned(),
        id(WORKSPACE),
        "missing".to_owned(),
        None,
    );
    assert_eq!(
        keys.set(&missing_model, NOW)
            .await
            .expect("missing model is a typed outcome"),
        SetPlatformKey::ModelNotFound
    );
    assert!(keys.list().await.expect("keys list").is_empty());

    let active = PlatformKeyInput::new(
        "anthropic".to_owned(),
        id(WORKSPACE),
        "claude-opus-5".to_owned(),
        None,
    );
    let SetPlatformKey::Set(key) = keys.set(&active, NOW).await.expect("default activates") else {
        panic!("a valid default must activate");
    };
    assert!(key.is_active());
    assert_eq!(key.model(), Some("claude-opus-5"));
    assert_eq!(
        models
            .delete(anthropic.id(), NOW)
            .await
            .expect("reference check reads"),
        DeleteModel::InUse
    );

    assert!(
        keys.deactivate("anthropic", NOW)
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
    fixtures.cleanup().await;
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
    base_url: String,
    name: String,
    database: Db,
}

impl Fixtures {
    async fn create() -> Self {
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_error| {
            panic!("{LANE_KNOB} is unset — run through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_admin_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        admin(&base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;
        let url = database_url(&base_url, &name);
        let migrator = open(&url, DbRole::Migrator).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema applies");
        drop(migrator);
        Self {
            database: open(&url, DbRole::Api).await,
            base_url,
            name,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(SEED)
            .execute(&mut *connection)
            .await
            .expect("the admin fixture seeds");
    }

    async fn revision(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar("SELECT revision FROM core.model_catalogue_revision WHERE id = 1")
            .fetch_one(&mut *connection)
            .await
            .expect("revision reads")
    }

    async fn cleanup(self) {
        drop(self.database);
        admin(
            &self.base_url,
            AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            )),
        )
        .await;
    }
}

fn database_url(base_url: &str, name: &str) -> String {
    let (prefix, tail) = base_url
        .rsplit_once('/')
        .expect("a Postgres URL has a database path");
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    if query.is_empty() {
        format!("{prefix}/{name}")
    } else {
        format!("{prefix}/{name}?{query}")
    }
}

async fn open(url: &str, role: DbRole) -> Db {
    let env = MapEnv::from_pairs(DbRole::ALL.iter().map(|each| (each.url_knob(), url)));
    Db::connect(&PoolConfig::resolve(&env, role).expect("the URL resolves"))
        .await
        .expect("the database accepts a connection")
}

async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane database is reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement runs");
    drop(connection);
    pool.close().await;
}
