//! One tenant, one workspace and its fleets, seeded on the real schema and
//! erased with the tenant when a suite ends.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id, mint_uuid7};
use afd_memory::Memories;
use afd_wire::memory::{MemoryDelta, Visibility};

/// The status every seeded fleet carries.
const FLEET_ACTIVE: &str = "active";

/// A workspace on the shared lane database, and the memory handle over it.
pub(crate) struct Workspace {
    lane: TestDatabase,
    pub(crate) database: Db,
    tenant: String,
    pub(crate) id: Uuid7,
    pub(crate) memories: Memories,
}

/// The two grants a seeded fleet holds.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Grants {
    pub(crate) read: bool,
    pub(crate) publish: bool,
}

impl Workspace {
    /// A tenant and a workspace with no fleets.
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let tenant = mint_id();
        let id = mint_uuid7();
        let mut connection = database.acquire().await.expect("an API connection");
        TenantSeed::new(&tenant)
            .insert(&mut *connection)
            .await
            .expect("the tenant seed");
        WorkspaceSeed::new(id.as_str(), &tenant)
            .insert(&mut *connection)
            .await
            .expect("the workspace seed");
        drop(connection);
        let memories = Memories::new(database.clone(), Entropy::new());
        Self {
            lane,
            database,
            tenant,
            id,
            memories,
        }
    }

    /// A fleet named `name` holding `grants`.
    pub(crate) async fn fleet(&self, name: &str, grants: Grants) -> Uuid7 {
        let fleet = mint_uuid7();
        FleetSeed::new(fleet.as_str(), self.id.as_str(), &self.tenant, FLEET_ACTIVE)
            .name(name)
            .memory(grants.read, grants.publish)
            .insert(&mut *self.database.acquire().await.expect("an API connection"))
            .await
            .expect("the fleet seeds");
        fleet
    }

    /// Erases everything the suite seeded, through the tenant's cascade.
    pub(crate) async fn cleanup(self) {
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *self.database.acquire().await.expect("an API connection"))
            .await
            .expect("the tenant erases");
        drop(self.memories);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

/// A delta under `key` with `visibility`.
pub(crate) fn delta(key: &str, category: &str, visibility: Visibility) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(format!("what {key} holds")),
        category: Cow::Owned(category.to_owned()),
        visibility,
    }
}
