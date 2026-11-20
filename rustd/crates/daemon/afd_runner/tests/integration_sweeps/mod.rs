//! Runner sweep transitions against the lane's migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

mod seed;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::liveness::Liveness;
use jiff::Timestamp;

/// Long ago, and after the never-seen sentinel.
const OLD: Timestamp = Timestamp::constant(1, 0);
/// Passes the lane's backlog may take to drain: far past any lane's runners
/// over the liveness batch.
const MAX_DRAIN_PASSES: usize = 64;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn liveness_converges_real_runner_rows() {
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let liveness = Liveness::new(fixture.database.clone(), Entropy::new());
    let first = liveness.sweep().await.expect("liveness sweeps real rows");
    assert!(
        first.scanned >= 2,
        "silent and idle draining hosts are actionable"
    );
    assert!(
        first.changed >= 3,
        "offline, automatic cordon and drain commit"
    );
    assert_eq!(fixture.runner_state(&fixture.draining).await, "drained");
    assert!(
        !fixture.thread_is_expired(&fixture.stale_fleet).await,
        "silence never backdates a live thread"
    );
    assert!(
        !fixture.thread_is_expired(&fixture.cordoned_fleet).await,
        "operator policy leaves the live deadline alone"
    );
    assert!(!fixture.thread_is_expired(&fixture.unknown_fleet).await);
    assert_eq!(fixture.offline_events().await, 1);
    // The episode's instant reaches the operator API the way every other
    // instant does: RFC 3339, in the form jiff writes.
    assert_eq!(fixture.offline_last_seen().await, OLD.to_string());

    // A pass takes one batch of due runners, and the lane may hold more than a
    // batch of other suites' stale rows; the repeat converges once they drain.
    converge(&liveness).await;
    assert_eq!(fixture.offline_events().await, 1);

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: Uuid7,
    workspace: Uuid7,
    stale: Uuid7,
    draining: Uuid7,
    cordoned: Uuid7,
    unknown: Uuid7,
    stale_fleet: Uuid7,
    cordoned_fleet: Uuid7,
    unknown_fleet: Uuid7,
    stale_run: Uuid7,
    cordoned_run: Uuid7,
    unknown_run: Uuid7,
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
            stale_run: mint_id(),
            cordoned_run: mint_id(),
            unknown_run: mint_id(),
            lane,
        }
    }

    async fn scalar<T>(&self, query: &'static str, value: &Uuid7) -> T
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

    async fn runner_state(&self, runner: &Uuid7) -> String {
        self.scalar("SELECT status FROM runner.runners WHERE id = $1", runner)
            .await
    }

    async fn thread_is_expired(&self, fleet: &Uuid7) -> bool {
        self.scalar(
            "SELECT run_expires_at < statement_timestamp() FROM thread.threads WHERE fleet_id = $1",
            fleet,
        )
        .await
    }

    async fn offline_events(&self) -> i64 {
        self.scalar(
            "SELECT count(*) FROM runner.history WHERE runner_id = $1 AND event_type = 'runner_offline'",
            &self.stale,
        ).await
    }

    async fn offline_last_seen(&self) -> String {
        self.scalar(
            "SELECT metadata->>'last_seen_at' FROM runner.history WHERE runner_id = $1 AND event_type = 'runner_offline'",
            &self.stale,
        ).await
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        // Runners belong to no tenant, so they go by their own ids.
        sqlx::query("DELETE FROM runner.runners WHERE id = ANY($1)")
            .bind(&[&self.stale, &self.draining, &self.cordoned, &self.unknown][..])
            .execute(&mut *connection)
            .await
            .expect("the scoped runners clean up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

async fn converge(liveness: &Liveness) {
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
}
