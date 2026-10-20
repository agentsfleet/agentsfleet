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

use afd_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TenantSeed, TestDatabase, WorkspaceSeed};
use afd_db::{Db, PoolConfig};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_fleet_lifecycle::Fleets;
use jiff::Timestamp;

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
pub(crate) const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The name the lane's tenant and workspace carry.
const FIXTURE_NAME: &str = "fixture";

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
        let queue = afd_dragonfly::test_util::connect_live(&dragonfly_config())
            .await
            .expect("the lane's Dragonfly must be reachable");
        Self::over(queue).await
    }

    /// The lane's database with a Dragonfly nobody answers on.
    ///
    /// For the proofs that are settled in Postgres before a purge reaches the
    /// queue — a refused purge never forgets a stream — so they run wherever the
    /// lane's Postgres does. The lazy manager opens no socket until asked.
    pub(crate) async fn without_queue() -> Self {
        let config = DragonflyConfig::from_url(DragonflyRole::Default, NOWHERE.to_owned());
        Self::over(Dragonfly::unreachable(&config).expect("a lazy manager opens no socket")).await
    }

    /// Seeds a tenant and one workspace and builds the store over `queue`.
    async fn over(queue: Dragonfly) -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
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
        WorkspaceSeed::new(workspace.as_str(), self.tenant.as_str())
            .name("second")
            .at(NOW)
            .insert(&mut *self.connection().await)
            .await
            .expect("seeding a second workspace");
        workspace
    }

    /// How many rows `fleet.fleets` holds for one workspace.
    ///
    /// The orphan check: after a purge this must be what it was before, and a
    /// response assertion alone cannot say that.
    pub(crate) async fn fleet_count(&self, workspace: &Uuid7) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE workspace_id = $1::uuid")
            .bind(workspace.as_str())
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("counting fleets")
    }

    /// One column of one fleet row, as text, or `None` when the row is gone.
    pub(crate) async fn fleet_column(&self, fleet: &Uuid7, column: &str) -> Option<String> {
        // The column name is a literal from this suite, never input.
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM fleet.fleets WHERE id = $1::uuid"
        ));
        sqlx::query_scalar::<_, Option<String>>(statement)
            .bind(fleet.as_str())
            .fetch_optional(&mut *self.connection().await)
            .await
            .expect("reading a fleet column")
            .flatten()
    }

    /// Drops the database. Dragonfly keys are namespaced per fleet and age out.
    pub(crate) async fn cleanup(self) {
        self.database.cleanup().await;
    }

    /// A pool as the lane's owner, for the scene a test sets around the code
    /// under test: rows `api_runtime` may not write, DDL, and the counts of
    /// tables it may not read. The caller closes it.
    ///
    /// Opened per call rather than held, so a lane that never sets such a
    /// scene keeps one pool's warm connections, not two.
    pub(crate) async fn owner(&self) -> Db {
        self.database.open(DbRole::Migrator, &[]).await
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
        TenantSeed::new(self.tenant.as_str())
            .name(FIXTURE_NAME)
            .at(NOW)
            .insert(&mut *connection)
            .await
            .expect("seeding a tenant");
        WorkspaceSeed::new(self.workspace.as_str(), self.tenant.as_str())
            .name(FIXTURE_NAME)
            .at(NOW)
            .insert(&mut *connection)
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
    Uuid7::encode(NOW, bytes).expect("a well-formed identifier")
}
