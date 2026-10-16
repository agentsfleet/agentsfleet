//! The lane's database, its Dragonfly, and the rows a fleet needs to exist.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated.
//!
//! # One isolation argument, now used on both sides
//!
//! Dragonfly never had a database-per-test equivalent, so every key this suite
//! touches is namespaced by a fleet identifier the test minted for itself: two
//! tests running at once cannot collide because neither can name the other's
//! fleet. That worked. Postgres is now isolated the same way and for the same
//! reason — see [`afd_db::test_util`] on what the per-test database cost and
//! what it was actually buying.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, PoolConfig};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_fleet_lifecycle::Fleets;
use sqlx::Row as _;

/// The environment knob naming the lane's Dragonfly.
const DRAGONFLY_URL_KNOB: &str = "TEST_DRAGONFLY_URL";

/// The environment knob naming its certificate authority, where the lane uses one.
const DRAGONFLY_CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// A Postgres nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so a
/// connection fails on refusal rather than waiting out a timeout.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// A Postgres connection string nothing listens on.
const NOWHERE_DATABASE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// The acquire budget knob a deployment sets, and a short value for it — not
/// so short the pool gives up before its first connect attempt answers, which
/// `afd_db` would classify as capacity rather than an unreachable datastore.
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";
const ACQUIRE_TIMEOUT_MS: &str = "50";

/// The instant every fixture row is stamped with.
pub(crate) const NOW_MS: i64 = 1_760_000_000_000;

/// The fixture WRITES, in a file of their own.
///
/// A child module so it reaches this one's constants: the instant it stamps
/// is the lane's.
mod seed;

/// A migrated database, a queue, and the store over both.
pub(crate) struct Lane {
    database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) queue: Dragonfly,
    pub(crate) fleets: Fleets,
    pub(crate) workspace: Uuid7,
    pub(crate) tenant: Uuid7,
}

impl Lane {
    /// Connects the queue and seeds a tenant and one workspace inside the
    /// lane's shared database.
    ///
    /// No `CREATE DATABASE` and no migration — the lane brought both. Isolation
    /// is the minted `tenant` and `workspace`, which is the same isolation the
    /// Dragonfly side has always relied on: keys namespaced by identifiers only
    /// this test can name.
    pub(crate) async fn create() -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let queue = afd_dragonfly::test_util::connect_live(&dragonfly_config())
            .await
            .expect("the lane's Dragonfly must be reachable");
        let fleets = Fleets::new(pool.clone(), queue.clone());

        let lane = Self {
            tenant: mint(),
            workspace: mint(),
            database,
            pool,
            queue,
            fleets,
        };
        lane.seed_tenant_and_workspace().await;
        lane
    }

    /// The same store, pointed at a Postgres nobody answers on.
    ///
    /// `Db::unreachable` opens no socket, so the store is built without a
    /// datastore anywhere near it and every acquire refuses as the transport
    /// class — the shape of the outage a wall's `hello` has to survive.
    pub(crate) fn with_dead_database() -> Fleets {
        let environment = MapEnv::from_pairs([
            (DbRole::Api.url_knob(), NOWHERE_DATABASE),
            (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
        ]);
        let dead = Db::unreachable(
            &PoolConfig::resolve(&environment, DbRole::Api)
                .expect("the fixture connection string is well formed"),
        );
        let config = DragonflyConfig::from_url(DragonflyRole::Default, NOWHERE.to_owned());
        let queue = Dragonfly::unreachable(&config).expect("a lazy manager opens no socket");
        Fleets::new(dead, queue)
    }

    /// A second workspace under the same tenant, for the cross-workspace proofs.
    pub(crate) async fn another_workspace(&self) -> Uuid7 {
        let workspace = mint();
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5)",
        )
        .bind(workspace.as_str())
        .bind(self.tenant.as_str())
        .bind("second")
        .bind("fixture")
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a second workspace");
        workspace
    }

    /// How many rows `core.fleets` holds for one workspace.
    ///
    /// The orphan check: after a purge this must be what it was before, and a
    /// response assertion alone cannot say that.
    pub(crate) async fn fleet_count(&self, workspace: &Uuid7) -> i64 {
        sqlx::query("SELECT count(*) FROM core.fleets WHERE workspace_id = $1::uuid")
            .bind(workspace.as_str())
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("counting fleets")
            .try_get::<i64, _>(0)
            .expect("a count is a bigint")
    }

    /// One column of one fleet row, as text, or `None` when the row is gone.
    pub(crate) async fn fleet_column(&self, fleet: &Uuid7, column: &str) -> Option<String> {
        // The column name is a literal from this suite, never input.
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM core.fleets WHERE id = $1::uuid"
        ));
        sqlx::query(statement)
            .bind(fleet.as_str())
            .fetch_optional(&mut *self.connection().await)
            .await
            .expect("reading a fleet column")
            .and_then(|row| row.try_get::<Option<String>, _>(0).ok().flatten())
    }

    /// The instant this suite stamps writes with.
    pub(crate) const fn now() -> UnixMillis {
        UnixMillis::from_millis(NOW_MS)
    }

    /// Drops the database. Dragonfly keys are namespaced per fleet and age out.
    pub(crate) async fn cleanup(self) {
        self.database.cleanup().await;
    }

    /// One pooled connection, for the fixture's own reads and writes.
    pub(crate) async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.pool
            .acquire()
            .await
            .expect("the fixture database must answer")
    }

    /// Seeds the tenant and the workspace every fixture fleet lands in.
    async fn seed_tenant_and_workspace(&self) {
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at) \
             VALUES ($1::uuid, 'fixture', $2, $2)",
        )
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");

        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($1::uuid, $2::uuid, 'fixture', 'fixture', $3)",
        )
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a workspace");
    }
}

/// The lane's Dragonfly configuration.
fn dragonfly_config() -> DragonflyConfig {
    let url = ProcessEnv.get(DRAGONFLY_URL_KNOB).unwrap_or_else(|| {
        panic!("{DRAGONFLY_URL_KNOB} is unset — run these through `make test-integration-rustd`")
    });
    DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(ProcessEnv.get(DRAGONFLY_CA_KNOB).map(Into::into))
        .with_request_timeout(Duration::from_secs(5))
}

/// A fresh identifier, so no two fixtures can name each other's rows.
pub(crate) fn mint() -> Uuid7 {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host draws entropy");
    Uuid7::encode(Lane::now(), bytes).expect("a well-formed identifier")
}
