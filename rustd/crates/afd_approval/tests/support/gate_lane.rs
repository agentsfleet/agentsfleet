//! The lane's database, its Redis, and the rows a gate needs to exist.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated.
//!
//! # Its own fixture, and not `afd_fleet`'s
//!
//! The crate under test does not depend on the runner plane, so neither does
//! its suite. Seeding a tenant, a workspace and a fleet is three statements;
//! borrowing them from `afd_fleet::tests` would put leases, money and policy
//! behind a test about a person answering a question.
//!
//! # One tenant, and a workspace and fleet per test
//!
//! The tenant is shared scaffolding: every test writes the same row under
//! `ON CONFLICT (id) DO NOTHING`, whichever runs first wins, and nothing
//! asserts over it. The workspace and fleet are minted per test, because tests
//! DO assert over those — a count of a fleet's events, a listing of a
//! workspace's queue — and on one shared database a fixed pair puts every
//! sibling's rows inside this test's answer. That was a real failure, three
//! runs in a row on two different tests, and it is the line: shared when every
//! writer writes the same row, minted the moment a test reads a set.
//!
//! There is no teardown. `TestDatabase::shared` owns no database to drop, so a
//! `cleanup` here would be a call every test makes that does nothing, implying
//! an isolation boundary that is not there.
//!
//! # The sweeper is global, so its tests take a lock
//!
//! `Inbox::expire` is a system-wide statement by design — `WHERE status =
//! pending AND timeout_at <= now`, no workspace or fleet in it — which is what
//! a sweeper must be in production and what no amount of identifier minting can
//! isolate. A test that sweeps therefore expires every OTHER test's lapsed
//! gate, and a test whose premise is a gate past its window finds it already
//! resolved. Those two groups take [`sweeper_exclusive`] and run one at a time;
//! everything else still runs concurrently.
//!
//! # Redis is here because an approval CONTINUES a run
//!
//! The resolve appends a stream entry for the event it unblocked. That is the
//! one reason this suite needs a queue, and it is why the fixture connects a
//! real one rather than a dead handle: a continuation nothing appended is the
//! failure the dimension exists to catch.
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

use afd_approval::Inbox;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_redis::config::{RedisConfig, RedisRole};
use sqlx::Row as _;

/// The environment knob naming the lane's Redis.
const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";

/// The environment knob naming its certificate authority.
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// Serialises the tests the global sweeper cannot be isolated from.
///
/// Held for the whole test body by anything that calls `Inbox::expire`, and by
/// anything that seeds a gate already past its window and expects to find it
/// PENDING. Both sides are needed: the lock exists to keep those two groups
/// apart, and a lock only one side takes keeps nothing apart at all.
///
/// A `tokio` mutex rather than the standard library's, because the guard is
/// held across `await` points, and it does not poison — one failing test leaves
/// the rest runnable rather than turning a single red into a suite of them.
pub(crate) async fn sweeper_exclusive() -> tokio::sync::MutexGuard<'static, ()> {
    static SWEEPER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    SWEEPER.lock().await
}

/// The instant every fixture row is stamped with.
pub(crate) const NOW_MS: i64 = 1_760_000_000_000;

/// The tenant every gate in this suite hangs from.
///
/// Fixed, readable and SHARED — see the module note. Version-7 shaped so the
/// schema's `ck_*_id_uuidv7` admits it.
const TENANT: &str = "0195b4ba-8d3a-7a11-8abc-000000000001";

/// How long a fixture gate waits before the sweeper may take it.
pub(crate) const WINDOW_MS: i64 = 60_000;

/// The kind a fixture gate carries.
///
/// Deliberately NOT `integration_grant`: that kind makes the resolve write a
/// second table, and these tests are about the gate row's own transition.
const KIND: &str = "repository_write";

/// A migrated database, a queue, and one fleet to hang gates from.
pub(crate) struct Lane {
    /// Held, not used: `TestDatabase::shared` is what resolves the lane's URL,
    /// and dropping it early would close nothing but would make the pool's
    /// provenance unreadable.
    _database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) inbox: Inbox,
    /// The fleet every seeded gate belongs to.
    pub(crate) fleet: Uuid7,
    /// Its workspace.
    pub(crate) workspace: Uuid7,
    /// Its tenant, shared by every lane in the suite.
    pub(crate) tenant: Uuid7,
}

impl Lane {
    /// A workspace and fleet of this test's OWN, under the shared tenant.
    ///
    /// The only constructor, because on one shared database every alternative
    /// is a race. A lane on fixed identifiers puts every test's gates in every
    /// other test's fleet: a count of that fleet's events reads a sibling's
    /// continuation, and a sweep over that workspace expires the gate a
    /// sibling is about to answer. Both are real failures this suite has had.
    /// Minting the pair costs three seeding statements and removes the class.
    pub(crate) async fn isolated() -> Self {
        Self::open(mint().as_str().to_owned(), mint().as_str().to_owned()).await
    }

    /// Opens the lane over `workspace` and `fleet`, seeding both idempotently.
    async fn open(workspace: String, fleet: String) -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let queue = afd_redis::test_util::connect_live(&redis_config())
            .await
            .expect("the lane's Redis must be reachable");

        let lane = Self {
            inbox: Inbox::new(pool.clone(), queue),
            _database: database,
            pool,
            fleet: Uuid7::parse(&fleet).expect("a fixture fleet id is well formed"),
            workspace: Uuid7::parse(&workspace).expect("a fixture workspace id is well formed"),
            tenant: Uuid7::parse(TENANT).expect("the fixture tenant id is well formed"),
        };
        lane.seed_fleet().await;
        lane
    }

    /// The instant this suite stamps writes with.
    pub(crate) const fn now() -> UnixMillis {
        UnixMillis::from_millis(NOW_MS)
    }

    /// Seeds one pending gate, returning its action id.
    ///
    /// `timeout_at` is a parameter because the sweeper tests need a deadline in
    /// the past and the race tests need one that has not arrived — a fixture
    /// that fixed it would make one of the two impossible to write.
    pub(crate) async fn seed_gate(&self, timeout_at: i64) -> String {
        let action = afd_db::test_util::mint_id();
        sqlx::query(
            "INSERT INTO core.fleet_approval_gates
               (id, fleet_id, workspace_id, action_id, tool_name, action_name,
                gate_kind, proposed_action, evidence, blast_radius, timeout_at,
                resolved_by, status, detail, created_at, updated_at, event_id,
                spend_count, spend_ceiling)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push',
                     $5, 'open a pull request', '{}'::jsonb, 'one repository',
                     $6, '', 'pending', '', $7, NULL, $8, 0, 32)",
        )
        .bind(mint().as_str())
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(&action)
        .bind(KIND)
        .bind(timeout_at)
        .bind(NOW_MS)
        .bind(afd_db::test_util::mint_id())
        .execute(&mut *self.connection().await)
        .await
        .expect("the gate row must insert");
        action
    }

    /// The status column of one gate, by action.
    pub(crate) async fn status_of(&self, action: &str) -> String {
        sqlx::query("SELECT status FROM core.fleet_approval_gates WHERE action_id = $1")
            .bind(action)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the gate row is readable")
            .try_get(0)
            .expect("a status is text")
    }

    /// One column of one gate's row, as text.
    pub(crate) async fn gate_column(&self, action: &str, column: &str) -> String {
        // The column name is a literal from this suite, never input.
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM core.fleet_approval_gates WHERE action_id = $1"
        ));
        sqlx::query(statement)
            .bind(action)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the gate row is readable")
            .try_get(0)
            .expect("the column must be readable as text")
    }

    /// How many events this lane's fleet holds.
    pub(crate) async fn event_count(&self) -> i64 {
        sqlx::query("SELECT count(*) FROM core.fleet_events WHERE fleet_id = $1::uuid")
            .bind(self.fleet.as_str())
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the count must run")
            .try_get(0)
            .expect("a count is a bigint")
    }

    /// One column of one event row, as text.
    pub(crate) async fn event_column(&self, event: &str, column: &str) -> Option<String> {
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM core.fleet_events \
             WHERE fleet_id = $1::uuid AND event_id = $2"
        ));
        sqlx::query(statement)
            .bind(self.fleet.as_str())
            .bind(event)
            .fetch_optional(&mut *self.connection().await)
            .await
            .expect("the event read must run")
            .and_then(|row| row.try_get::<Option<String>, _>(0).ok().flatten())
    }

    /// One pooled connection, for the fixture's own reads and writes.
    async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.pool
            .acquire()
            .await
            .expect("the fixture database must answer")
    }

    /// Seeds the tenant, workspace and fleet every gate hangs from.
    async fn seed_fleet(&self) {
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, 'fixture', $2, $2)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");

        sqlx::query(
            // The NAME carries the id, not a fixed word: `uq_workspaces_tenant_id_name`
            // is unique per tenant, and every isolated lane shares the tenant —
            // so a constant name would collide where the `ON CONFLICT (id)` arm
            // cannot see it.
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
             VALUES ($1::uuid, $2::uuid, $1::text, 'fixture', $3)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a workspace");

        sqlx::query(
            "INSERT INTO core.fleets
               (id, workspace_id, tenant_id, name, source_markdown, config_json,
                status, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'fixture-fleet', '# fixture',
                     '{}'::jsonb, 'active', $4, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a fleet");
    }
}

/// The lane's Redis configuration.
fn redis_config() -> RedisConfig {
    let url = std::env::var(REDIS_URL_KNOB).unwrap_or_else(|_unset| {
        panic!("{REDIS_URL_KNOB} is unset — run these through `make test-integration-rustd`")
    });
    RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(REDIS_CA_KNOB).ok().map(Into::into))
        .with_request_timeout(Duration::from_secs(5))
}

/// A fresh identifier, so no two fixtures can name each other's rows.
///
/// Through `afd_db`'s minting helper rather than a local entropy draw: this
/// crate has no reason to depend on the crypto crate, and the helper already
/// shapes a value both `ck_*_id_uuidv7` and [`Uuid7`] accept.
pub(crate) fn mint() -> Uuid7 {
    Uuid7::parse(&afd_db::test_util::mint_id()).expect("a minted identifier is well formed")
}
