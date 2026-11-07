//! A stopped fleet and its owned parents isolate each ledger test.

use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_db::{Db, DbRole};
use jiff::Timestamp;

use super::Fallible;

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
    pub(crate) async fn seed() -> Fallible<Self> {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let seeded = Self {
            handle: lane,
            database,
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
        };
        let mut connection = seeded.database.acquire().await?;
        TenantSeed::new(&seeded.tenant)
            .at(SEEDED_AT)
            .insert(&mut *connection)
            .await?;
        WorkspaceSeed::new(&seeded.workspace, &seeded.tenant)
            .at(SEEDED_AT)
            .insert(&mut *connection)
            .await?;
        drop(connection);
        seeded.seed_fleet().await?;
        Ok(seeded)
    }

    async fn seed_fleet(&self) -> Fallible {
        FleetSeed::new(&self.fleet, &self.workspace, &self.tenant, FLEET_STOPPED)
            .name("admission-fixture-fleet")
            .at(SEEDED_AT)
            .native_setup()
            .insert(&mut *self.database.acquire().await?)
            .await?;
        Ok(())
    }

    /// Removes the fleet's rows, parents last. Best effort: a connection that
    /// cannot be had leaves the rows, which no other case addresses.
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
            if let Ok(mut connection) = self.database.acquire().await {
                let _removed = sqlx::query(table).bind(id).execute(&mut *connection).await;
            }
        }
        self.handle.cleanup().await;
    }
}
