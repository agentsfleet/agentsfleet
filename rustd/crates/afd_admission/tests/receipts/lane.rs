//! A stopped fleet and its owned parents isolate each ledger test.

use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_db::{Db, DbRole};
use jiff::Timestamp;

/// The instant the seeded rows carry.
const SEEDED_AT: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The status that keeps every lease path off the fixture fleet.
const FLEET_STOPPED: &str = "stopped";

/// One fleet, stopped so no lease path picks it up, and the rows above it.
pub(crate) struct Lane {
    handle: TestDatabase,
    pub(super) database: Db,
    tenant: String,
    pub(super) workspace: String,
    pub(super) fleet: String,
}

impl Lane {
    /// Seeds a tenant, a workspace and a stopped fleet on the lane's database.
    pub(crate) async fn seed() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let seeded = Self {
            handle: lane,
            database,
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
        };
        let mut connection = seeded
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        TenantSeed::new(&seeded.tenant)
            .at(SEEDED_AT)
            .insert(&mut *connection)
            .await
            .expect("seeding a tenant");
        WorkspaceSeed::new(&seeded.workspace, &seeded.tenant)
            .at(SEEDED_AT)
            .insert(&mut *connection)
            .await
            .expect("seeding a workspace");
        drop(connection);
        seeded.seed_fleet().await;
        seeded
    }

    async fn seed_fleet(&self) {
        FleetSeed::new(&self.fleet, &self.workspace, &self.tenant, FLEET_STOPPED)
            .name("admission-fixture-fleet")
            .at(SEEDED_AT)
            .native_setup()
            .insert(&mut *self.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("seeding a fleet");
    }

    /// Removes the fleet's rows, parents last.
    pub(crate) async fn cleanup(self) {
        for (table, id) in [
            ("DELETE FROM fleet.fleets WHERE id = $1::uuid", &self.fleet),
            (
                "DELETE FROM identity.workspaces WHERE id = $1::uuid",
                &self.workspace,
            ),
            (
                "DELETE FROM identity.tenants WHERE id = $1::uuid",
                &self.tenant,
            ),
        ] {
            let _removed = sqlx::query(table)
                .bind(id)
                .execute(&mut *self.database.acquire().await.expect("a pooled connection"))
                .await;
        }
        self.handle.cleanup().await;
    }
}
