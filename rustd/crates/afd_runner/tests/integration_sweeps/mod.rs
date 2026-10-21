//! Runner sweep transitions against the lane's migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

mod seed;

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::liveness::Liveness;
use afd_runner::sweep::retention::Retention;
use jiff::Timestamp;

/// Long before any retention window, and after the never-seen sentinel.
const OLD: Timestamp = Timestamp::constant(1, 0);
/// Passes the lane's backlog may take to drain: far past any lane's runners
/// over the liveness batch.
const MAX_DRAIN_PASSES: usize = 64;
static SWEEP_LANE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn liveness_and_retention_converge_real_runner_rows() {
    let _lane = SWEEP_LANE.lock().await;
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
    // The episode's instant reaches the operator API the way every other
    // instant does: RFC 3339, in the form jiff writes.
    assert_eq!(fixture.offline_last_seen().await, OLD.to_string());

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
            "SELECT status FROM runner.runners WHERE id = $1::uuid",
            runner,
        )
        .await
    }

    async fn slot_is_expired(&self, fleet: &str) -> bool {
        self.scalar(
            "SELECT lease_expires_at < statement_timestamp() FROM runner.fleet_slots WHERE fleet_id = $1::uuid",
            fleet,
        )
        .await
    }

    async fn offline_events(&self) -> i64 {
        self.scalar(
            "SELECT count(*) FROM runner.events WHERE runner_id = $1::uuid AND event_type = 'runner_offline'",
            &self.stale,
        ).await
    }

    async fn offline_last_seen(&self) -> String {
        self.scalar(
            "SELECT metadata->>'last_seen_at' FROM runner.events WHERE runner_id = $1::uuid AND event_type = 'runner_offline'",
            &self.stale,
        ).await
    }

    async fn lease_exists(&self, lease: &str) -> bool {
        self.scalar(
            "SELECT EXISTS(SELECT 1 FROM runner.leases WHERE id = $1::uuid)",
            lease,
        )
        .await
    }

    async fn lease_state(&self, lease: &str) -> String {
        self.scalar(
            "SELECT status FROM runner.leases WHERE id = $1::uuid",
            lease,
        )
        .await
    }

    async fn runner_event_exists(&self, event: &str) -> bool {
        self.scalar(
            "SELECT EXISTS(SELECT 1 FROM runner.events WHERE id = $1::uuid)",
            event,
        )
        .await
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        // Runners belong to no tenant, so they go by their own ids.
        sqlx::query("DELETE FROM runner.runners WHERE id = ANY($1::uuid[])")
            .bind([&self.stale, &self.draining, &self.cordoned, &self.unknown])
            .execute(&mut *connection)
            .await
            .expect("the scoped runners clean up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

/// Past the retention window by a day: due for deletion on any pass.
const AGED: Timestamp = Timestamp::constant(86_400, 0);

/// Rows one retention cycle may delete from one table: its batch ceiling
/// times its batch limit, as the module note on saturation states them.
const CYCLE_CEILING: i64 = 8 * 1_000;

/// Rows past [`CYCLE_CEILING`]: what is left for the following cycle.
const LEFT_OVER: i64 = 500;

/// A cycle that fills every batch it is allowed comes back in a minute; the
/// one after, which drains what was left, idles again.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_backlog_past_one_cycle_brings_the_next_cycle_forward() {
    // Every retention pass in the lane deletes from the same tables; the
    // sweep above runs one, so the two take turns.
    let _lane = SWEEP_LANE.lock().await;
    let fixture = Fixture::create().await;
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    afd_db::test_util::RunnerSeed::new(
        &fixture.stale,
        afd_runner::sql::RUNNER_STATUS_ACTIVE,
        crate::rows::POLICY,
    )
    .at(OLD)
    .insert(&mut *connection)
    .await
    .expect("the runner row seeds");
    sqlx::query(
        "INSERT INTO runner.events (id, runner_id, event_type, metadata, created_at) \
         SELECT uuidv7(), $1::uuid, 'lease_acquired', '{}', $2::timestamptz \
         FROM generate_series(1, $3::bigint)",
    )
    .bind(&fixture.stale)
    .bind(jiff_sqlx::ToSqlx::to_sqlx(AGED))
    .bind(CYCLE_CEILING + LEFT_OVER)
    .execute(&mut *connection)
    .await
    .expect("the aged backlog seeds");
    drop(connection);

    let retention = Retention::new(fixture.database.clone());
    let idle = retention.interval();
    let first = retention.sweep().await.expect("retention sweeps real rows");
    assert!(
        first.changed >= u64::try_from(CYCLE_CEILING).expect("a row count"),
        "the cycle deleted its whole ceiling: {first:?}"
    );
    assert!(
        retention.interval() < idle,
        "a cycle that hit its ceiling comes back sooner"
    );

    retention
        .sweep()
        .await
        .expect("retention sweeps the remainder");
    assert_eq!(
        fixture
            .scalar::<i64>(
                "SELECT count(*) FROM runner.events WHERE runner_id = $1::uuid",
                &fixture.stale,
            )
            .await,
        0,
        "the second cycle drained what the first left"
    );
    assert_eq!(retention.interval(), idle, "a drained cycle idles again");
    fixture.cleanup().await;
}
