//! The second daemon replica, and the expiry it re-admits work after.
//!
//! Its own pool and Dragonfly connection over the lane both replicas share, so
//! the two meet only where two deployed replicas meet: rows and queue keys.

use super::fixture::Running;
use afd_core::{clock, id::Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::{Db, config::DbRole, test_util::TestDatabase};
use afd_fleet::lease::{Billed, Kind, Leases};

const ORIGINAL: &str = "\
SELECT e.event_id, l.status FROM runner.leases l JOIN fleet.events e ON e.id = l.fleet_event_id
WHERE l.id = $1::uuid";

pub(super) struct Replica {
    lane: TestDatabase,
    database: Db,
    pub(super) leases: Leases,
}

impl Replica {
    pub(super) async fn start() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let leases = Leases::new(
            database.clone(),
            crate::queue::connect().await,
            Entropy::new(),
        );
        Self {
            lane,
            database,
            leases,
        }
    }

    /// B's poll: win the lapsed claim, take the dead holder's event, write a lease.
    pub(super) async fn readmit(&self, running: &Running) -> (String, Uuid7) {
        let acquired = crate::seed::select_fleet_within_rotations(
            &self.leases,
            &running.runner,
            &running.fleet,
        )
        .await
        .expect("the lapsed fleet is offered to the second replica");
        assert_eq!(
            acquired.kind,
            Kind::Reclaim,
            "the expired holder's event is taken back, never pulled fresh and billed twice"
        );
        let carried = acquired
            .reused
            .as_ref()
            .expect("a reclaim carries the original billing");
        let tenant = Uuid7::parse(&carried.tenant_id).expect("carried tenant");
        let issued = self
            .leases
            .issue(
                &running.runner,
                &acquired,
                Billed {
                    tenant_id: &tenant,
                    posture: &carried.posture,
                    provider: crate::seed::PROVIDER,
                    model: &carried.model,
                },
                clock::now(),
            )
            .await
            .expect("issue reaches Postgres")
            .expect("the freed slot admits the re-leased event");
        (acquired.event_id, issued.lease_id)
    }

    pub(super) async fn stop(self) {
        self.database.close().await;
        self.lane.cleanup().await;
    }
}

/// Lapse both halves of A's lease at database time, as a stalled holder would.
pub(super) async fn expire(running: &Running) {
    running
        .fixtures
        .lapse_holder(running.lease.as_str(), &running.fleet)
        .await;
}

/// The event and status of A's original lease row.
pub(super) async fn original(running: &Running) -> (String, String) {
    sqlx::query_as(ORIGINAL)
        .bind(running.lease.as_str())
        .fetch_one(&mut *running.fixtures.database.acquire().await.expect("read"))
        .await
        .expect("the original lease row")
}
