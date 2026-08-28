//! The lane's two datastores, and the rows every event hangs from.
//!
//! `core.fleet_events` has foreign keys into `core.fleets` and
//! `core.workspaces`, so no history test can seed a single row without first
//! seeding a tenant, a workspace and a fleet. That chain is here rather than in
//! each suite, because getting it wrong fails with a constraint violation that
//! names a table the test never mentions.
//!
//! # Isolation, and what replaces it
//!
//! Postgres is the SHARED lane database — `TestDatabase::shared`, never a
//! database per test, for the reason `make/test-integration-rustd.mk` states at
//! length: a database per test meant applying forty-seven schema files per test.
//! Redis has no database-per-test equivalent at all. What replaces isolation in
//! both is that every fixture mints its OWN workspace and fleet identifiers, so
//! two suites running in parallel address disjoint rows and disjoint keys.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![expect(
    dead_code,
    reason = "test support: shared by two test binaries, each using a subset"
)]

use std::time::Duration;

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use sqlx::Row as _;

/// The knob `make test-integration-rustd` exports the lane's Redis under.
const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";

/// The knob carrying the lane's CA bundle, where the lane speaks TLS.
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// How long anything crossing a datastore is given before the test fails.
pub(crate) const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// A tenant, a workspace and a fleet nothing else in the suite addresses, over
/// both live datastores.
pub(crate) struct EventsLane {
    lane: TestDatabase,
    pub(crate) database: Db,
    pub(crate) queue: Redis,
    pub(crate) tenant: String,
    pub(crate) workspace: String,
    pub(crate) fleet: String,
}

impl EventsLane {
    /// Opens both datastores and seeds the row chain a fleet event needs.
    pub(crate) async fn open() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let queue = Redis::connect(&redis_config())
            .await
            .expect("the lane's Redis must be reachable");

        let seeded = Self {
            lane,
            database,
            queue,
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
        };
        seeded.seed_fleet().await;
        seeded
    }

    /// The lane's Redis configuration, for a caller that opens its own handle.
    pub(crate) fn redis() -> RedisConfig {
        redis_config()
    }

    /// A pooled connection, or a failed test.
    pub(crate) async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.database
            .acquire()
            .await
            .expect("the lane database must hand over a connection")
    }

    /// Writes one `core.fleet_events` row, as a runner's report leaves it.
    ///
    /// `created_at` is a parameter rather than a clock read: every claim these
    /// suites make about a `since` window is a claim about ROW TIMESTAMPS, and
    /// a fixture that stamped rows with "now" could only ever seed one instant.
    pub(crate) async fn seed_event(&self, event_id: &str, created_at: i64) {
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.fleet_events
               (fleet_id, workspace_id, event_id, actor, event_type, status,
                request_json, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3, 'steer:api', 'chat', 'completed',
                     '{}'::jsonb, $4, $4)
             ON CONFLICT (fleet_id, event_id) DO NOTHING",
        )
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(event_id)
        .bind(created_at)
        .execute(&mut *connection)
        .await
        .expect("seeding a fleet event");
    }

    /// Whether the lane holds a row under `event_id`.
    pub(crate) async fn has_event(&self, event_id: &str) -> bool {
        let mut connection = self.connection().await;
        let count: i64 = sqlx::query(
            "SELECT count(*) FROM core.fleet_events
             WHERE fleet_id = $1::uuid AND event_id = $2",
        )
        .bind(self.fleet.as_str())
        .bind(event_id)
        .fetch_one(&mut *connection)
        .await
        .expect("the count must run")
        .try_get(0)
        .expect("count answers a bigint");
        count > 0
    }

    /// Drops the database this handle created, if it created one.
    ///
    /// A no-op on the shared lane, which this fixture does not own. Called
    /// unconditionally anyway: the call is how a test says it is finished.
    pub(crate) async fn cleanup(self) {
        self.lane.cleanup().await;
    }

    /// Seeds the tenant, workspace and fleet every event hangs from.
    async fn seed_fleet(&self) {
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, 'events-fixture', $2, $2)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");

        sqlx::query(
            // The NAME carries the id rather than a fixed word:
            // `uq_workspaces_tenant_id_name` is unique per tenant, and a
            // constant name would collide where the `ON CONFLICT (id)` arm
            // cannot see it.
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
             VALUES ($1::uuid, $2::uuid, $1::text, 'events-fixture', $3)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a workspace");

        sqlx::query(
            "INSERT INTO core.fleets
               (id, workspace_id, tenant_id, name, source_markdown, config_json,
                status, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'events-fixture-fleet',
                     '# fixture', '{}'::jsonb, 'active', $4, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a fleet");
    }
}

/// The instant the fixture rows are stamped with.
///
/// Fixed rather than "now": these rows are the SCAFFOLD, never the subject, and
/// a fixed stamp keeps a `since` window's arithmetic readable against the row
/// timestamps a test chooses for itself.
const SEED_MS: i64 = 1_700_000_000_000;

/// The lane's Redis configuration.
fn redis_config() -> RedisConfig {
    let url = std::env::var(REDIS_URL_KNOB).unwrap_or_else(|_unset| {
        panic!("{REDIS_URL_KNOB} is unset — run these through `make test-integration-rustd`")
    });
    RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(REDIS_CA_KNOB).ok().map(Into::into))
        .with_request_timeout(Duration::from_secs(5))
}
