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
use afd_db::test_util::{TestDatabase, mint_id};
use afd_memory::Memories;
use afd_wire::memory::{MemoryDelta, Visibility};

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
        let id = Uuid7::parse(&mint_id()).expect("a minted id is canonical");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Shared memory', 1, 1) \
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($2::uuid, $1::uuid, 'shared-memory', 'user_memory_suite', 1)",
        )
        .bind(&tenant)
        .bind(id.as_str())
        .execute(&mut *database.acquire().await.expect("an API connection"))
        .await
        .expect("the tenant and workspace seed");
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
        let fleet = Uuid7::parse(&mint_id()).expect("a minted id is canonical");
        sqlx::query(
            "INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, status, \
                memory_reads_workspace, memory_publishes_workspace, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, '# fixture', '{}', 'active', $5, $6, 1, 1)",
        )
        .bind(fleet.as_str())
        .bind(self.id.as_str())
        .bind(&self.tenant)
        .bind(name)
        .bind(grants.read)
        .bind(grants.publish)
        .execute(&mut *self.database.acquire().await.expect("an API connection"))
        .await
        .expect("the fleet seeds");
        fleet
    }

    /// Erases everything the suite seeded, through the tenant's cascade.
    pub(crate) async fn cleanup(self) {
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
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
