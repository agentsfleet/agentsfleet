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
    DefaultSeed, FleetSeed, GrantSeed, ModelSeed, OwnedRows, RunnerPolicy, RunnerSeed, SEEDED_AT,
    TenantSeed, TestDatabase, UserSeed, WalletSeed, WorkspaceSeed, mint_id,
};

/// The balance the wallet case seeds.
const BALANCE_NANOS: i64 = 42;

/// The model every case prices, under its own provider.
const MODEL: &str = "seed-model";

/// The context ceiling the catalogue row carries.
const CAP: i32 = 1_000;

/// Prices no real catalogue row carries, so the read-back is unmistakable, in
/// nanos per million tokens: uncached input, cached input, output.
const INPUT_NANOS_PER_MTOK: i64 = 7;
const CACHED_INPUT_NANOS_PER_MTOK: i64 = 3;
const OUTPUT_NANOS_PER_MTOK: i64 = 11;

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
    ModelSeed::new(&model_row, &scene.provider, MODEL, CAP)
        .rates(
            INPUT_NANOS_PER_MTOK,
            CACHED_INPUT_NANOS_PER_MTOK,
            OUTPUT_NANOS_PER_MTOK,
        )
        .verified_at(SEEDED_AT)
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
        "SELECT output_nanos_per_mtok FROM model.platform_model_prices \
         WHERE platform_model_id = $1::uuid",
    )
    .bind(&model_row)
    .fetch_one(&mut *connection)
    .await
    .expect("the priced row reads back");
    assert_eq!(output, OUTPUT_NANOS_PER_MTOK);

    DefaultSeed::release(&mut *connection, &scene.provider, &scene.workspace)
        .await
        .expect("the default releases");
    ModelSeed::remove(&mut *connection, &model_row)
        .await
        .expect("the priced row removes once nothing names it");
    drop(connection);
    let defaults = "SELECT count(*) FROM model.platform_defaults WHERE provider = $1";
    let models = "SELECT count(*) FROM model.platform_models WHERE id = $1::uuid";
    assert_eq!(scene.count(defaults, &scene.provider).await, 0);
    assert_eq!(scene.count(models, &model_row).await, 0);
}

/// The status the seeded fleet and runner both hold.
const ACTIVE: &str = "active";

/// The status a revoked grant holds.
const REVOKED: &str = "revoked";

/// Why the seeded runner may not take work, as the column holds it.
const DEGRADED: &str = "seed fixture: cgroup controller missing";

/// A host policy every runner row carries.
const POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

/// The optional builders a suite reaches for write the column each one names:
/// a revoked grant carries its revocation, a degraded runner its reason, and a
/// back-dated person the instant it was handed rather than the database's.
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL — run via make test-integration-rustd"]
async fn the_optional_builders_write_the_column_they_name() {
    let scene = Scene::seed().await;
    let (fleet, user, runner) = (mint_id(), mint_id(), mint_id());
    let mut owned = OwnedRows::new(&TestDatabase::shared());
    owned.record("DELETE FROM runner.runners WHERE id = $1::uuid", &runner);
    owned.record(
        "DELETE FROM connector.integration_grants WHERE fleet_id = $1::uuid",
        &fleet,
    );
    owned.record("DELETE FROM identity.users WHERE id = $1::uuid", &user);
    owned.record("DELETE FROM fleet.fleets WHERE id = $1::uuid", &fleet);
    let mut connection = scene.database.acquire().await.expect("a connection");
    FleetSeed::new(&fleet, &scene.workspace, &scene.tenant, ACTIVE)
        .insert(&mut *connection)
        .await
        .expect("the fleet seed");
    let grant = GrantSeed::new(&fleet, &scene.provider, REVOKED)
        .revoked(SEEDED_AT)
        .insert(&mut *connection)
        .await
        .expect("the grant seed");
    UserSeed::new(&user, &scene.tenant, &user, "seed@example.test")
        .at(SEEDED_AT)
        .insert(&mut *connection)
        .await
        .expect("the person seed");
    RunnerSeed::new(&runner, ACTIVE, POLICY)
        .degraded(DEGRADED)
        .insert(&mut *connection)
        .await
        .expect("the runner seed");

    let revoked: Option<jiff_sqlx::Timestamp> = sqlx::query_scalar(
        "SELECT revoked_at FROM connector.integration_grants WHERE id = $1::uuid",
    )
    .bind(&grant)
    .fetch_one(&mut *connection)
    .await
    .expect("the grant reads back");
    let joined: jiff_sqlx::Timestamp =
        sqlx::query_scalar("SELECT created_at FROM identity.users WHERE id = $1::uuid")
            .bind(&user)
            .fetch_one(&mut *connection)
            .await
            .expect("the person reads back");
    let reason: Option<String> =
        sqlx::query_scalar("SELECT degraded_reason FROM runner.runners WHERE id = $1::uuid")
            .bind(&runner)
            .fetch_one(&mut *connection)
            .await
            .expect("the runner reads back");
    drop(connection);

    assert_eq!(revoked.map(jiff_sqlx::Timestamp::to_jiff), Some(SEEDED_AT));
    assert_eq!(joined.to_jiff(), SEEDED_AT);
    assert_eq!(reason.as_deref(), Some(DEGRADED));
    owned.finish().await.expect("the seeded rows remove");
}
