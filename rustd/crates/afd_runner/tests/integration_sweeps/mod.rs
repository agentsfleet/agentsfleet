//! Runner sweep transitions against the lane's migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

mod seed;

use afd_core::clock;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::liveness::Liveness;
use afd_runner::sweep::retention::Retention;

const OLD: i64 = 1;
const NEVER_SEEN: i64 = 0;
/// Passes the lane's backlog may take to drain: far past any lane's runners
/// over the liveness batch.
const MAX_DRAIN_PASSES: usize = 64;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn liveness_and_retention_converge_real_runner_rows() {
    // The liveness pass expires EVERY abandoned active lease in the lane, and
    // the reclaim fixtures strand one on purpose; run beside them, this pass
    // flips that lease before their reclaim pass can surface it.
    let _lane = crate::integration_reclaim_faults::RECLAIM_LANE.lock().await;
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let liveness = Liveness::new(fixture.database.clone(), Entropy::new());
    let first = liveness.sweep().await.expect("liveness sweeps real rows");
    assert!(first.scanned >= 4, "the four fixture runners are scanned");
    assert!(first.changed >= 4, "the four stale fixture rows converge");
    assert_eq!(fixture.runner_state(&fixture.draining).await, "drained");
    assert!(fixture.slot_is_expired(&fixture.stale_fleet).await);
    assert!(fixture.slot_is_expired(&fixture.cordoned_fleet).await);
    assert!(!fixture.slot_is_expired(&fixture.unknown_fleet).await);
    assert_eq!(fixture.offline_events().await, 1);

    // A pass takes one batch of due runners, and the lane may hold more than a
    // batch of other suites' stale rows; the repeat converges once they drain.
    let mut passes = 0;
    while liveness
        .sweep()
        .await
        .expect("a repeated pass converges")
        .changed
        > 0
    {
        passes += 1;
        assert!(passes < MAX_DRAIN_PASSES, "the lane's due runners drain");
    }
    assert_eq!(fixture.offline_events().await, 1);

    let retention = Retention::new(fixture.database.clone());
    let retained = retention.sweep().await.expect("retention sweeps real rows");
    assert!(retained.changed >= 4);
    assert!(!fixture.lease_exists(&fixture.reported_lease).await);
    assert_eq!(fixture.lease_state(&fixture.unknown_lease).await, "expired");
    assert!(!fixture.runner_event_exists(&fixture.old_event).await);

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: String,
    stale: String,
    draining: String,
    cordoned: String,
    unknown: String,
    stale_fleet: String,
    cordoned_fleet: String,
    unknown_fleet: String,
    stale_lease: String,
    cordoned_lease: String,
    unknown_lease: String,
    reported_lease: String,
    stale_event: String,
    cordoned_event: String,
    unknown_event: String,
    reported_event: String,
    old_event: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: mint_id(),
            stale: mint_id(),
            draining: mint_id(),
            cordoned: mint_id(),
            unknown: mint_id(),
            stale_fleet: mint_id(),
            cordoned_fleet: mint_id(),
            unknown_fleet: mint_id(),
            stale_lease: mint_id(),
            cordoned_lease: mint_id(),
            unknown_lease: mint_id(),
            reported_lease: mint_id(),
            stale_event: format!("evt_{}", mint_id()),
            cordoned_event: format!("evt_{}", mint_id()),
            unknown_event: format!("evt_{}", mint_id()),
            reported_event: format!("evt_{}", mint_id()),
            old_event: mint_id(),
            lane,
        }
    }

    async fn scalar<T>(&self, query: &'static str, value: &str) -> T
    where
        T: for<'row> sqlx::Decode<'row, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(query)
            .bind(value)
            .fetch_one(&mut *connection)
            .await
            .expect("the scoped assertion reads")
    }

    async fn runner_state(&self, runner: &str) -> String {
        self.scalar(
            "SELECT admin_state FROM fleet.runners WHERE id = $1::uuid",
            runner,
        )
        .await
    }

    async fn slot_is_expired(&self, fleet: &str) -> bool {
        let leased_until: i64 = self
            .scalar(
                "SELECT leased_until FROM fleet.runner_affinity WHERE fleet_id = $1::uuid",
                fleet,
            )
            .await;
        leased_until < clock::now().as_millis()
    }

    async fn offline_events(&self) -> i64 {
        self.scalar(
            "SELECT count(*) FROM fleet.runner_events WHERE runner_id = $1::uuid AND event_type = 'runner_offline'",
            &self.stale,
        ).await
    }

    async fn lease_exists(&self, lease: &str) -> bool {
        self.scalar(
            "SELECT EXISTS(SELECT 1 FROM fleet.runner_leases WHERE id = $1::uuid)",
            lease,
        )
        .await
    }

    async fn lease_state(&self, lease: &str) -> String {
        self.scalar(
            "SELECT status FROM fleet.runner_leases WHERE id = $1::uuid",
            lease,
        )
        .await
    }

    async fn runner_event_exists(&self, event: &str) -> bool {
        self.scalar(
            "SELECT EXISTS(SELECT 1 FROM fleet.runner_events WHERE id = $1::uuid)",
            event,
        )
        .await
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
