//! Readiness-index recovery over real fleet rows and Dragonfly streams.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TestDatabase, mint_id};
use afd_dragonfly::{Dragonfly, FleetStreams, ReadyIndex};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::reclaim::Reclaim;
use afd_state::sql::FLEET_STATUS_ACTIVE;

use crate::support::{RING_QUIET_FOR, connect_redis, heard_ring, listen_for_rings};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn reclaim_restores_only_a_fleet_with_deliverable_work() {
    let _lane = crate::integration_reclaim_faults::RECLAIM_LANE.lock().await;
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let queue = connect_redis().await;
    let streams = FleetStreams::new(queue.clone());
    fixture.open_streams(&streams).await;

    let (_hub, mut rings) = listen_for_rings(&fixture.deliverable).await;
    let reclaim = Reclaim::new(fixture.database.clone(), queue.clone(), "sweep-test");
    let swept = reclaim.sweep().await.expect("the reclaim pass completes");
    assert!(swept.scanned >= 2, "the two fixture fleets are scanned");
    assert!(
        swept.changed >= 1,
        "at least the fixture's deliverable fleet is re-marked"
    );

    // Each fleet's own mark is read rather than sampled: a peek is a random
    // sample of one partition, and once the lane holds more marks there than
    // the sample takes, a mark that IS present can be missed.
    let index = ReadyIndex::new(queue.clone());
    let remarked = index
        .token_for(&fixture.deliverable)
        .await
        .expect("the readiness index is readable")
        .expect("a lost readiness hint is reconstructed from the stream");
    // A backstop the rotation reaches in its turn: ringing it would send a
    // poll every pass to each partition a running fleet holds.
    assert!(
        !heard_ring(&mut rings, &remarked, RING_QUIET_FOR).await,
        "the sweep's re-mark rings no doorbell"
    );
    assert!(
        index
            .token_for(&fixture.empty)
            .await
            .expect("the readiness index is readable")
            .is_none(),
        "an empty stream does not create a false readiness hint"
    );

    fixture.forget_streams(&queue, &streams).await;
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
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             SELECT $2::uuid, id, 'reclaim', 'test', 1 FROM tenant",
        )
        .bind(&self.tenant)
        .bind(&self.workspace)
        .execute(&mut *connection)
        .await
        .expect("the active fleet rows seed");
        for (fleet, name) in [(&self.deliverable, "deliverable"), (&self.empty, "empty")] {
            FleetSeed::new(fleet, &self.workspace, &self.tenant, FLEET_STATUS_ACTIVE)
                .name(name)
                .at(1)
                .insert(&mut *connection)
                .await
                .expect("the active fleet rows seed");
        }
    }

    /// Gives the deliverable fleet a stream holding one undelivered entry, and
    /// the empty fleet a stream holding none.
    async fn open_streams(&self, streams: &FleetStreams) {
        streams
            .ensure_group(&self.deliverable)
            .await
            .expect("the deliverable group exists");
        streams
            .append(&self.deliverable, &[("type", "reclaim-test")])
            .await
            .expect("the undelivered entry appends");
        streams
            .ensure_group(&self.empty)
            .await
            .expect("the empty group exists");
    }

    /// Clears the readiness mark the sweep wrote and both fixture streams.
    async fn forget_streams(&self, queue: &Dragonfly, streams: &FleetStreams) {
        ReadyIndex::new(queue.clone())
            .force_clear(&self.deliverable)
            .await
            .expect("the readiness fixture cleans up");
        streams
            .forget(&self.deliverable)
            .await
            .expect("the deliverable stream cleans up");
        streams
            .forget(&self.empty)
            .await
            .expect("the empty stream cleans up");
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
