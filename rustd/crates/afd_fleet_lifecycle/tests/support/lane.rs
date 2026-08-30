//! The lane's database, its Redis, and the rows an install needs to exist.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated.
//!
//! # One isolation argument, now used on both sides
//!
//! Redis never had a database-per-test equivalent, so every key this suite
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
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_fleet_lifecycle::Fleets;
use afd_redis::config::{RedisConfig, RedisRole};
use afd_redis::{Redis, fleet_stream_key};
use sqlx::Row as _;

/// The environment knob naming the lane's Redis.
const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";

/// The environment knob naming its certificate authority, where the lane uses one.
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// A Postgres nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so a
/// connection fails on refusal rather than waiting out a timeout.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// The instant every fixture row is stamped with.
pub(crate) const NOW_MS: i64 = 1_760_000_000_000;

/// The platform library entry every lane in this suite installs from.
///
/// Fixed and SHARED, because the row is scaffolding rather than test data:
/// every `Lane::create` writes the same id, the same markdown and the same
/// hash, so the second lane to run wants exactly the row the first one wrote.
/// The seed says so with `ON CONFLICT (id) DO NOTHING`. Minting one per lane
/// instead buys nothing and costs an identifier threaded through every install.
///
/// This holds only while the row stays immutable. A test needing the STORED
/// entry to differ — other markdown under the same id — must seed its own id:
/// `DO NOTHING` would keep the first writer's row and the test would assert
/// against it.
pub(crate) const LIBRARY_ID: &str = "daily-digest";

/// The visibility a published library entry carries.
const VISIBILITY_PUBLIC: &str = "public";

/// The `SKILL.md` the fixture entry stores.
pub(crate) const SKILL_MD: &str =
    "---\nname: daily-digest\ndescription: A digest.\nversion: 1.0.0\n---\nProse.\n";

/// The `TRIGGER.md` beside it.
pub(crate) const TRIGGER_MD: &str = "---\nname: daily-digest\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n";

/// A second `TRIGGER.md`, naming the same fleet with a different ceiling.
///
/// Used where a test needs the stored configuration to CHANGE observably — the
/// name has to match or the cross-file check refuses it.
pub(crate) const TRIGGER_MD_EDITED: &str = "---\nname: daily-digest\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 5.0\n---\n";

/// A THIRD `TRIGGER.md`, for the two-writer race.
///
/// The race needs both writers to carry a document that differs from what the
/// row holds. The `If-Match` predicate is content-addressed — it hashes
/// `source_markdown` and `trigger_markdown` — so a writer that resends the
/// STORED bytes changes nothing, leaves the hash where it was, and the other
/// writer's guard still matches. Both then report success, and the assertion
/// that exactly one wins fails, intermittently, on whichever future finished
/// first. That is the fixture being wrong about the race, not the predicate
/// being wrong about the write: an idempotent write is not a version moving.
pub(crate) const TRIGGER_MD_RIVAL: &str = "---\nname: daily-digest\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 9.0\n---\n";

/// A migrated database, a queue, and the store over both.
pub(crate) struct Lane {
    database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) queue: Redis,
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
    /// Redis side has always relied on: keys namespaced by identifiers only
    /// this test can name.
    pub(crate) async fn create() -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let queue = afd_redis::test_util::connect_live(&redis_config())
            .await
            .expect("the lane's Redis must be reachable");
        let fleets = Fleets::new(pool.clone(), queue.clone(), Entropy::new());

        let lane = Self {
            tenant: mint(),
            workspace: mint(),
            database,
            pool,
            queue,
            fleets,
        };
        lane.seed_tenant_and_workspace().await;
        lane.seed_library_entry(LIBRARY_ID, SKILL_MD, Some(TRIGGER_MD))
            .await;
        lane
    }

    /// The same store, pointed at a Redis nobody answers on.
    ///
    /// The transport-failure seam for the install guarantee: `Redis::unreachable`
    /// builds the manager WITHOUT opening a socket, so every command fails as a
    /// TRANSPORT error rather than a command error — which is the class the
    /// install retries, and the only one it should.
    pub(crate) fn with_dead_queue(&self) -> Fleets {
        let config = RedisConfig::from_url(RedisRole::Default, NOWHERE.to_owned())
            .with_request_timeout(Duration::from_millis(250));
        let dead = Redis::unreachable(&config).expect("a lazy manager opens no socket");
        Fleets::new(self.pool.clone(), dead, Entropy::new())
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
    /// The orphan check: after a rolled-back install this must be what it was
    /// before, and a response assertion alone cannot say that.
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

    /// Whether Redis holds a consumer group for this fleet's event stream.
    ///
    /// The install's whole guarantee, asserted against the queue rather than
    /// against the response: `XINFO GROUPS` on a stream with none answers an
    /// empty list, and on a key that does not exist it errors.
    pub(crate) async fn has_consumer_group(&self, fleet: &Uuid7) -> bool {
        let key = fleet_stream_key(fleet.as_str());
        let mut command = redis::cmd("XINFO");
        command.arg("GROUPS").arg(&key);
        self.queue
            .command::<Vec<redis::Value>>("XINFO", &key, &command)
            .await
            .is_ok_and(|groups| !groups.is_empty())
    }

    /// Puts a plain string where a fleet's event stream would go.
    ///
    /// The COMMAND-failure seam, as distinct from the transport one: Redis is up
    /// and answers, and `XGROUP CREATE … MKSTREAM` against a key holding a
    /// string is a `WRONGTYPE` error. Retrying it three more times would answer
    /// the same, which is exactly what the install must not spend the budget on.
    pub(crate) async fn occupy_stream_key(&self, fleet: &Uuid7) {
        let key = fleet_stream_key(fleet.as_str());
        let mut command = redis::cmd("SET");
        command.arg(&key).arg("not-a-stream");
        self.queue
            .command::<String>("SET", &key, &command)
            .await
            .expect("occupying the stream key");
    }

    /// Seeds one platform library entry, idempotently.
    ///
    /// `ON CONFLICT (id) DO NOTHING`, because every lane seeds this row into one
    /// shared database. Whichever lane runs FIRST therefore decides the stored
    /// content — correct for [`LIBRARY_ID`], which is identical every time, and
    /// wrong for a caller wanting different markdown under a reused id.
    pub(crate) async fn seed_library_entry(
        &self,
        id: &str,
        skill_markdown: &str,
        trigger_markdown: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO core.fleet_library \
               (id, name, description, source_repo, source_path, source_ref, \
                required_credentials, required_credentials_reasons, required_tools, \
                network_hosts, visibility, content_hash, skill_markdown, trigger_markdown, \
                created_at, updated_at) \
             VALUES ($1, $1, 'fixture', 'repo', 'path', 'main', \
                     '[]'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, \
                     $2, $3, $4, $5, $6, $6) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(VISIBILITY_PUBLIC)
        .bind(format!("sha256:{id}"))
        .bind(skill_markdown)
        .bind(trigger_markdown)
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a library entry");
    }

    /// The instant this suite stamps writes with.
    pub(crate) const fn now() -> UnixMillis {
        UnixMillis::from_millis(NOW_MS)
    }

    /// Drops the database. Redis keys are namespaced per fleet and age out.
    pub(crate) async fn cleanup(self) {
        self.database.cleanup().await;
    }

    /// One pooled connection, for the fixture's own reads and writes.
    async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.pool
            .acquire()
            .await
            .expect("the fixture database must answer")
    }

    /// Seeds the tenant and the workspace every fixture install lands in.
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
pub(crate) fn mint() -> Uuid7 {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host draws entropy");
    Uuid7::encode(Lane::now(), bytes).expect("a well-formed identifier")
}
