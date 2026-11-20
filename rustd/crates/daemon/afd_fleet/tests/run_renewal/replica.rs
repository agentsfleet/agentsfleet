//! The second daemon replica, and the expiry it re-admits work after.
//!
//! Its own pool over the lane both replicas share, and its own in-process bus.
//! The bus is isolated deliberately: the claim is that the second replica
//! re-admits a lapsed run from the rows alone, so nothing the first one
//! publishes may reach it.

use super::fixture::Running;
use af_core::{clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use afd_db::{Db, config::DbRole, test_util::TestDatabase};
use afd_fleet::run::{Billed, Kind, Runs};

const ORIGINAL: &str = "\
SELECT e.id, l.status FROM thread.runs l JOIN thread.turns e ON e.id = l.turn_id
WHERE l.id = $1";

pub(super) struct Replica {
    lane: TestDatabase,
    database: Db,
    pub(super) runs: Runs,
}

impl Replica {
    pub(super) async fn start() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let runs = Runs::new(
            database.clone(),
            afd_bus::test_util::detached()
                .expect("an in-process bus opens")
                .0,
            Entropy::new(),
        );
        Self {
            lane,
            database,
            runs,
        }
    }

    /// B's poll: win the lapsed claim, take the dead holder's event, write a run.
    pub(super) async fn readmit(&self, running: &Running) -> (Uuid7, Uuid7) {
        let acquired = crate::seed::select_fleet(&self.runs, &running.runner, &running.fleet)
            .await
            .expect("the lapsed fleet is assigned to the second replica");
        assert_eq!(
            acquired.kind,
            Kind::Reclaim,
            "the expired holder's event is taken back, never pulled fresh and billed twice"
        );
        let tenant: Uuid7 = sqlx::query_scalar("SELECT tenant_id FROM thread.runs WHERE id = $1")
            .bind(&running.run)
            .fetch_one(&mut *running.fixtures.database.acquire().await.expect("read"))
            .await
            .expect("the original run row names its tenant");
        let issued = self
            .runs
            .issue(
                &running.runner,
                &acquired,
                Billed {
                    tenant_id: &tenant,
                    posture: crate::seed::POSTURE,
                    provider: crate::seed::PROVIDER,
                    model: crate::seed::MODEL,
                },
                clock::now(),
            )
            .await
            .expect("issue reaches Postgres")
            .expect("the freed slot admits the re-held event");
        (acquired.turn_id, issued.run_id)
    }

    pub(super) async fn stop(self) {
        self.database.close().await;
        self.lane.cleanup().await;
    }
}

/// Lapse both halves of A's run at database time, as a stalled holder would.
pub(super) async fn expire(running: &Running) {
    running
        .fixtures
        .lapse_holder(&running.run, &running.fleet)
        .await;
}

/// The event and status of A's original run row.
pub(super) async fn original(running: &Running) -> (Uuid7, String) {
    sqlx::query_as(ORIGINAL)
        .bind(&running.run)
        .fetch_one(&mut *running.fixtures.database.acquire().await.expect("read"))
        .await
        .expect("the original run row")
}
