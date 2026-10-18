//! The wallet, priced-model and claimed-default seeds write the rows they
//! name, and the two deployment-wide ones remove exactly what they wrote.
//!
//! Each case mints its own tenant, workspace and provider, so no other suite's
//! rows are in reach.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{
    DefaultSeed, PlatformModelSeed, Prices, SEEDED_AT, TenantSeed, TestDatabase, WalletSeed,
    WorkspaceSeed, mint_id,
};

/// The balance the wallet case seeds.
const BALANCE_NANOS: i64 = 42;

/// The model every case prices, under its own provider.
const MODEL: &str = "seed-model";

/// The context ceiling the catalogue row carries.
const CAP: i32 = 1_000;

/// Prices no real catalogue row carries, so the read-back is unmistakable.
const PRICES: Prices = Prices {
    input_nanos_per_mtok: 7,
    cached_input_nanos_per_mtok: 3,
    output_nanos_per_mtok: 11,
};

/// A tenant and workspace of one case's own, and a provider nobody else names.
struct Scene {
    database: Db,
    provider: String,
    tenant: String,
    workspace: String,
}

impl Scene {
    async fn seed() -> Self {
        let database = TestDatabase::shared().open(DbRole::Api, &[]).await;
        let id = mint_id().replace('-', "");
        let scene = Self {
            database,
            provider: format!("seed{}", &id[id.len() - 12..]),
            tenant: mint_id(),
            workspace: mint_id(),
        };
        let mut connection = scene.database.acquire().await.expect("a connection");
        TenantSeed::new(&scene.tenant)
            .insert(&mut *connection)
            .await
            .expect("the tenant seed");
        WorkspaceSeed::new(&scene.workspace, &scene.tenant)
            .insert(&mut *connection)
            .await
            .expect("the workspace seed");
        scene
    }

    fn default_seed(&self) -> DefaultSeed<'_> {
        DefaultSeed {
            provider: &self.provider,
            source_workspace_id: &self.workspace,
            model: MODEL,
            context_cap_tokens: CAP,
            created_at: SEEDED_AT,
        }
    }

    async fn count(&self, statement: &'static str, key: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query_scalar(statement)
            .bind(key)
            .fetch_one(&mut *connection)
            .await
            .expect("the count runs")
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL — run via make test-integration-rustd"]
async fn a_wallet_seed_holds_the_balance_it_was_given() {
    let scene = Scene::seed().await;
    let mut connection = scene.database.acquire().await.expect("a connection");
    WalletSeed::new(&scene.tenant, BALANCE_NANOS)
        .at(SEEDED_AT)
        .insert(&mut *connection)
        .await
        .expect("the wallet seed");

    let balance: i64 =
        sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1::uuid")
            .bind(&scene.tenant)
            .fetch_one(&mut *connection)
            .await
            .expect("the wallet reads back");
    assert_eq!(balance, BALANCE_NANOS);
    assert!(
        WalletSeed::new(&scene.tenant, BALANCE_NANOS)
            .insert(&mut *connection)
            .await
            .is_err(),
        "a tenant holds one wallet, so a second seed is refused"
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL — run via make test-integration-rustd"]
async fn a_claimed_default_names_the_priced_row_and_both_are_removable() {
    let scene = Scene::seed().await;
    let mut connection = scene.database.acquire().await.expect("a connection");
    let model_row = mint_id();
    PlatformModelSeed::new(&model_row, &scene.provider, MODEL, CAP, PRICES)
        .at(SEEDED_AT)
        .insert(&mut *connection)
        .await
        .expect("the priced row seeds");

    assert!(
        scene
            .default_seed()
            .claim(&mut *connection)
            .await
            .expect("the claim runs")
    );
    assert!(
        !scene
            .default_seed()
            .claim(&mut *connection)
            .await
            .expect("the claim runs"),
        "a provider holds one default, so the second claim answers that it wrote nothing"
    );
    let named: String = sqlx::query_scalar(
        "SELECT platform_model_id::text FROM model.platform_defaults WHERE provider = $1",
    )
    .bind(&scene.provider)
    .fetch_one(&mut *connection)
    .await
    .expect("the default reads back");
    assert_eq!(
        named, model_row,
        "the default names the priced row, not a zero-priced one"
    );
    let output: i64 = sqlx::query_scalar(
        "SELECT output_nanos_per_mtok FROM model.platform_models WHERE id = $1::uuid",
    )
    .bind(&model_row)
    .fetch_one(&mut *connection)
    .await
    .expect("the priced row reads back");
    assert_eq!(output, PRICES.output_nanos_per_mtok);

    DefaultSeed::release(&mut *connection, &scene.provider, &scene.workspace)
        .await
        .expect("the default releases");
    PlatformModelSeed::remove(&mut *connection, &model_row)
        .await
        .expect("the priced row removes once nothing names it");
    drop(connection);
    let defaults = "SELECT count(*) FROM model.platform_defaults WHERE provider = $1";
    let models = "SELECT count(*) FROM model.platform_models WHERE id = $1::uuid";
    assert_eq!(scene.count(defaults, &scene.provider).await, 0);
    assert_eq!(scene.count(models, &model_row).await, 0);
}
