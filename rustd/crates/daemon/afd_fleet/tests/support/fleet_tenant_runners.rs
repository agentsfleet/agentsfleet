//! Tenant-owned runner rows, for the suites that need a host a tenant holds.
//!
//! `Runners::register` enrols platform runners until the tenant routes land,
//! and a runner's tenant is fixed at insert, so a tenant-owned row is written
//! directly. Each helper borrows its connection for one statement, as the other
//! support modules do, so a test never holds a connection across `cleanup`.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_wire::runner::{NetworkPolicy, SandboxTier};
use afd_db::test_util::{ModelSeed, TenantSeed, mint_id};
use afd_runner::spelling::{policy_wire, tier_wire};
use jiff_sqlx::ToSqlx as _;

use crate::support::Fixtures;

/// A well-formed runner row. `$3` is its tenant, NULL for a platform runner; `$4` to
/// `$7` are the spellings the application stores, bound from its own constants so
/// this fixture cannot drift from them: the schema holds no list to refuse a stale one.
const INSERT_RUNNER: &str = "INSERT INTO runner.runners \
    (id, host_id, token_hash, sandbox_tier, status, labels, network_policy, \
     registry_allowlist, worker_count, last_seen_at, lease_version, created_at, \
     updated_at, tenant_id, platform_owned) \
    VALUES ($1, 'tenant-box', $2, $4, $5, '[]'::jsonb, $6, '[]'::jsonb, 1, $7, 0, \
            now(), now(), $3, $8)";

const INSERT_RECEIPT: &str = "INSERT INTO runner.registration_receipts \
    (id, tenant_id, operation_id, request_digest, runner_id, created_at) \
    VALUES ($1, $2, $3, 'digest', $4, now())";

impl Fixtures {
    /// A tenant row named after its own id.
    pub(crate) async fn seed_tenant(&self) -> Uuid7 {
        let tenant = mint_id();
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        TenantSeed::new(&tenant)
            .insert(&mut *connection)
            .await
            .expect("the tenant row must insert");
        tenant
    }

    /// A runner row owned by `tenant`, or a platform runner for `None`.
    pub(crate) async fn insert_runner(&self, tenant: Option<&Uuid7>) -> Uuid7 {
        self.insert_owned_runner(tenant.is_none(), tenant)
            .await
            .expect("a runner row of a valid shape must insert")
    }

    /// A runner row stating `platform_owned` and `tenant` as given, answering
    /// what the database said, so a test can offer it an inconsistent pair.
    ///
    /// # Errors
    /// The database's own refusal, as [`Fixtures::set_runner_tenant`].
    pub(crate) async fn insert_owned_runner(
        &self,
        platform_owned: bool,
        tenant: Option<&Uuid7>,
    ) -> Result<Uuid7, sqlx::Error> {
        let runner = mint_id();
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(INSERT_RUNNER)
            .bind(&runner)
            .bind(runner.as_str())
            .bind(tenant)
            .bind(tier_wire(SandboxTier::LandlockFull))
            .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
            .bind(policy_wire(NetworkPolicy::AllowAll))
            .bind(afd_runner::sql::NEVER_SEEN.to_sqlx())
            .bind(platform_owned)
            .execute(&mut *connection)
            .await?;
        Ok(runner)
    }

    /// Sets a runner's ownership flag, answering what the database said.
    ///
    /// # Errors
    /// The database's own refusal, as [`Fixtures::set_runner_tenant`].
    pub(crate) async fn set_runner_platform_owned(
        &self,
        runner: &Uuid7,
        platform_owned: bool,
    ) -> Result<(), sqlx::Error> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("UPDATE runner.runners SET platform_owned = $2 WHERE id = $1")
            .bind(runner)
            .bind(platform_owned)
            .execute(&mut *connection)
            .await
            .map(|_| ())
    }

    /// Sets a runner's tenant, answering what the database said.
    ///
    /// # Errors
    /// The database's own refusal, which a test reads to prove it was the
    /// trigger that refused and not some other failure.
    pub(crate) async fn set_runner_tenant(
        &self,
        runner: &Uuid7,
        tenant: Option<&Uuid7>,
    ) -> Result<(), sqlx::Error> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("UPDATE runner.runners SET tenant_id = $2 WHERE id = $1")
            .bind(runner)
            .bind(tenant)
            .execute(&mut *connection)
            .await
            .map(|_| ())
    }

    /// Renames a runner's host, an update that leaves its tenant alone.
    pub(crate) async fn rename_runner_host(&self, runner: &Uuid7, host: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("UPDATE runner.runners SET host_id = $2 WHERE id = $1")
            .bind(runner)
            .bind(host)
            .execute(&mut *connection)
            .await
            .expect("an update that leaves the tenant alone must succeed");
    }

    /// Assigns a runner a platform decision model, answering what the database said.
    ///
    /// # Errors
    /// The database's own refusal, as [`Fixtures::set_runner_tenant`].
    pub(crate) async fn set_runner_decision_model(
        &self,
        runner: &Uuid7,
        model: &Uuid7,
    ) -> Result<(), sqlx::Error> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("UPDATE runner.runners SET decision_platform_model_id = $2 WHERE id = $1")
            .bind(runner)
            .bind(model)
            .execute(&mut *connection)
            .await
            .map(|_| ())
    }

    /// A platform catalogue model no other test names, for a runner to reference.
    pub(crate) async fn seed_platform_model(&self) -> Uuid7 {
        let model = mint_id();
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        ModelSeed::new(&model, "anthropic", model.as_str(), 200_000)
            .insert(&mut *connection)
            .await
            .expect("the platform model must seed");
        model
    }

    /// Removes a model [`Fixtures::seed_platform_model`] seeded, once no runner names it.
    pub(crate) async fn remove_platform_model(&self, model: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        ModelSeed::remove(&mut *connection, model)
            .await
            .expect("the seeded platform model must remove");
    }

    /// A registration receipt for `tenant` that names `runner`.
    pub(crate) async fn insert_registration_receipt(&self, tenant: &Uuid7, runner: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(INSERT_RECEIPT)
            .bind(mint_id())
            .bind(tenant)
            .bind(mint_id())
            .bind(runner)
            .execute(&mut *connection)
            .await
            .expect("the receipt must insert");
    }

    /// How many registration receipts `tenant` holds.
    pub(crate) async fn registration_receipts(&self, tenant: &Uuid7) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT count(*) FROM runner.registration_receipts WHERE tenant_id = $1")
            .bind(tenant)
            .fetch_one(&mut *connection)
            .await
            .expect("the receipt count must read")
    }

    /// The runners among `runners` that still have a row, in id order.
    pub(crate) async fn surviving_runners(&self, runners: &[Uuid7]) -> Vec<Uuid7> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT id FROM runner.runners WHERE id = ANY($1) ORDER BY id")
            .bind(runners)
            .fetch_all(&mut *connection)
            .await
            .expect("the survivors must read")
    }

    /// Deletes a runner row. A tenant's runners go with the tenant instead.
    pub(crate) async fn delete_runner(&self, runner: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("DELETE FROM runner.runners WHERE id = $1")
            .bind(runner)
            .execute(&mut *connection)
            .await
            .expect("a runner row this test inserted must delete");
    }

    /// Deletes a tenant: erasure's root, which takes the tenant's runners with it.
    pub(crate) async fn delete_tenant(&self, tenant: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
            .bind(tenant)
            .execute(&mut *connection)
            .await
            .expect("erasing the tenant must run");
    }
}
