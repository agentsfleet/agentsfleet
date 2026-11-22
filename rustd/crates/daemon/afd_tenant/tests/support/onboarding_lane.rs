//! The lane's database and the rows the onboarding signals read.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated. Nothing here creates or migrates one.
//!
//! # Every identifier is minted, and none of these rows is shared
//!
//! The onboarding signals read five tables scoped by workspace and tenant.
//! Those rows are per-test DATA rather than scaffolding — a second test seeding
//! the same workspace would be asserting against the first one's fleet — so
//! every identifier here is minted and nothing carries an `ON CONFLICT` arm to
//! absorb a collision that should never happen.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed};
use afd_tenant::onboarding::Onboarding;
use jiff::Timestamp;

/// The instant every fixture identifier is minted at, and the fleet stamped
/// with. The rows this crate writes are stamped by the database clock.
pub(crate) const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The status the seeded fleet carries.
const FLEET_ACTIVE: &str = "active";

/// A migrated database, the store over it, and the rows one workspace needs.
pub(crate) struct Lane {
    database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) onboarding: Onboarding,
    /// The workspace this test's checklist describes.
    pub(crate) workspace: Uuid7,
    /// Its tenant, which the model signal is scoped by.
    pub(crate) tenant: Uuid7,
}

impl Lane {
    /// Seeds a tenant and a workspace inside the shared database.
    pub(crate) async fn create() -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let onboarding = Onboarding::new(pool.clone());

        let lane = Self {
            database,
            pool,
            onboarding,
            workspace: mint(),
            tenant: mint(),
        };
        lane.seed_workspace().await;
        lane
    }

    /// Puts one fleet in the workspace, so `has_fleet` turns true.
    pub(crate) async fn seed_fleet(&self) {
        FleetSeed::new(&mint(), &self.workspace, &self.tenant, FLEET_ACTIVE)
            .name("fixture")
            .at(NOW)
            .insert(&mut *self.connection().await)
            .await
            .expect("seeding a fleet");
    }

    /// Gives the tenant its own model and selects it, so `model_configured` is
    /// true without depending on whatever platform default the lane happens to
    /// hold. The row is written selected; the secret reference is inert here,
    /// since nothing in this crate resolves it. A tenant seeds one.
    pub(crate) async fn seed_tenant_model(&self, model: &str) {
        sqlx::query(
            "INSERT INTO model.tenant_models
               (id, tenant_id, model_name, secret_ref, selected, created_at, updated_at)
             VALUES ($2, $1, $3, 'fixture', true, now(), now())",
        )
        .bind(&self.tenant)
        .bind(mint())
        .bind(model)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a tenant model selection");
    }

    /// Drops nothing — the lane's database is shared. Kept so every suite here
    /// ends the same way its siblings do.
    pub(crate) async fn cleanup(self) {
        self.database.cleanup().await;
    }

    /// One pooled connection, for the fixture's own writes.
    async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.pool
            .acquire()
            .await
            .expect("the fixture database must answer")
    }

    /// Seeds the tenant and the workspace every read keys on.
    async fn seed_workspace(&self) {
        let mut connection = self.connection().await;
        TenantSeed::new(&self.tenant)
            .name("fixture")
            .insert(&mut *connection)
            .await
            .expect("seeding a tenant");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("fixture")
            .insert(&mut *connection)
            .await
            .expect("seeding a workspace");
    }
}

/// A fresh identifier, so no two fixtures can name each other's rows.
pub(crate) fn mint() -> Uuid7 {
    let mut bytes = [0u8; af_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host draws entropy");
    Uuid7::encode(NOW, bytes).expect("a well-formed identifier")
}
