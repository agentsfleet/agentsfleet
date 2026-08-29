//! Readiness-index recovery over real fleet rows and Redis streams.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_redis::{FleetStreams, ReadyIndex};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::reclaim::Reclaim;

use crate::support::connect_redis;

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn reclaim_restores_only_a_fleet_with_deliverable_work() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let queue = connect_redis().await;
    let streams = FleetStreams::new(queue.clone());
    streams
        .ensure_group(&fixture.deliverable)
        .await
        .expect("the deliverable group exists");
    streams
        .append(&fixture.deliverable, &[("type", "reclaim-test")])
        .await
        .expect("the undelivered entry appends");
    streams
        .ensure_group(&fixture.empty)
        .await
        .expect("the empty group exists");

    let reclaim = Reclaim::new(fixture.database.clone(), queue.clone(), "sweep-test");
    let swept = reclaim.sweep().await.expect("the reclaim pass completes");
    assert!(swept.scanned >= 2, "the two fixture fleets are scanned");
    assert!(
        swept.changed >= 1,
        "at least the fixture's deliverable fleet is re-marked"
    );

    let marked = ReadyIndex::new(queue.clone())
        .peek(100)
        .await
        .expect("the readiness index is readable");
    assert!(
        marked
            .iter()
            .any(|ready| ready.fleet_id == fixture.deliverable),
        "a lost readiness hint is reconstructed from the stream"
    );
    assert!(
        marked.iter().all(|ready| ready.fleet_id != fixture.empty),
        "an empty stream does not create a false readiness hint"
    );

    ReadyIndex::new(queue.clone())
        .force_clear(&fixture.deliverable)
        .await
        .expect("the readiness fixture cleans up");
    streams
        .forget(&fixture.deliverable)
        .await
        .expect("the deliverable stream cleans up");
    streams
        .forget(&fixture.empty)
        .await
        .expect("the empty stream cleans up");
    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: String,
    deliverable: String,
    empty: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: mint_id(),
            deliverable: mint_id(),
            empty: mint_id(),
            lane,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Reclaim sweep', 1, 1) \
               RETURNING id \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               SELECT $2::uuid, id, 'reclaim', 'test', 1 FROM tenant \
               RETURNING id, tenant_id \
             ) \
             INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             SELECT $3::uuid, id, tenant_id, 'deliverable', '# fixture', '{}'::jsonb, \
                    'active', 1, 1 FROM workspace \
             UNION ALL \
             SELECT $4::uuid, id, tenant_id, 'empty', '# fixture', '{}'::jsonb, \
                    'active', 1, 1 FROM workspace",
        )
        .bind(&self.tenant)
        .bind(&self.workspace)
        .bind(&self.deliverable)
        .bind(&self.empty)
        .execute(&mut *connection)
        .await
        .expect("the active fleet rows seed");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
