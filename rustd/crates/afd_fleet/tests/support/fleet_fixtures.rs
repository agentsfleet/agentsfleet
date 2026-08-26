//! A schema-loaded database, and the runner store over it.
//!
//! # Why a database per test
//!
//! Every assertion here is about a ROW — what enrolment wrote, what a beat
//! moved. Sharing one database would make each test's precondition depend on
//! the previous test's cleanup, which is how a suite starts passing in one
//! order and failing in another.
//!
//! # A note on where this belongs
//!
//! `afd_state` and `afd_db` each carry a near-identical creator, and
//! `afd_state`'s own copy already records that the right home for all of them
//! is `afd_db::test_util`, behind the feature that exists for exactly this.
//! This is a third copy and it is deliberate rather than unnoticed: moving all
//! three is a refactor across two suites this milestone does not touch, and
//! consolidating them from here would put a green integration lane at risk for
//! no runner-plane benefit. It stays a follow-up, named in the same words.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
// Two test binaries include this module by `#[path]`, so each compiles its own
// copy and uses a subset of it — what the row suite never calls is not dead
// code, it is the beat suite's half.
#![allow(
    dead_code,
    reason = "test support: shared by two test binaries, each using a subset"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::env::MapEnv;
use afd_crypto::entropy::Entropy;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::{Db, Migrator};
use afd_fleet::Runners;
use afd_fleet::gate::Gates;
use afd_fleet::lease::Leases;
use afd_redis::Redis;
use sqlx::{AssertSqlSafe, Row as _};

/// The lane's admin connection URL.
const LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Distinguishes databases created by one process, combined with the process
/// id so two lanes on one host cannot collide either.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// A schema-loaded database created for one test and dropped with it.
pub(crate) struct Fixtures {
    base_url: String,
    name: String,
    pub(crate) database: Db,
    queue: Option<Redis>,
    runners: Runners,
}

impl Fixtures {
    /// Creates a database, migrates it, and opens the api-role pool.
    ///
    /// No Redis. Most suites here assert on ROWS and never read the queue, and
    /// connecting one anyway is not free: each is a TLS handshake, and a suite
    /// running its tests in parallel opened enough at once to time out against
    /// a healthy server. Use [`Fixtures::create_with_queue`] where the queue is
    /// actually read.
    pub(crate) async fn create() -> Self {
        install_subscriber();
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_error| {
            panic!("{LANE_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_fleet_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        // The name is a process id and a counter, never input — which is what
        // makes interpolating it safe, since Postgres does not bind
        // identifiers. `AssertSqlSafe` is sqlx 0.9 asking that at the type
        // level; every other statement in this crate is a literal.
        admin(&base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;

        let url = database_url(&base_url, &name);
        let migrator = open(&url, DbRole::Migrator).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema must apply to a fresh database");
        drop(migrator);

        let database = open(&url, DbRole::Api).await;
        Self {
            runners: Runners::new(database.clone(), Entropy::new()),
            database,
            queue: None,
            base_url,
            name,
        }
    }

    /// The same, plus the lane's Redis.
    ///
    /// Shared rather than per-test: Redis has no database-per-test equivalent,
    /// and the keys these suites touch are namespaced by the fleet ids each
    /// test declares for itself.
    pub(crate) async fn create_with_queue() -> Self {
        let mut fixtures = Self::create().await;
        fixtures.queue = Some(crate::queue::connect().await);
        fixtures
    }

    /// The store under test.
    pub(crate) const fn runners(&self) -> &Runners {
        &self.runners
    }

    /// One text column of the runner's row.
    ///
    /// Read through `::text` so a caller asserts on the STORED spelling rather
    /// than on whatever this suite would have decoded it into — which is the
    /// point of a row-shape assertion.
    pub(crate) async fn runner_column(&self, runner: &str, column: &str) -> Option<String> {
        let statement = AssertSqlSafe(format!(
            "SELECT {column}::text FROM fleet.runners WHERE id = $1::uuid"
        ));
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(statement)
            .bind(runner)
            .fetch_one(&mut *connection)
            .await
            .expect("the runner row must exist")
            .try_get::<Option<String>, _>(0)
            .expect("the column must be readable as text")
    }

    /// The event types written for a runner, oldest first.
    pub(crate) async fn events(&self, runner: &str) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "SELECT event_type FROM fleet.runner_events \
             WHERE runner_id = $1::uuid ORDER BY created_at, id",
        )
        .bind(runner)
        .fetch_all(&mut *connection)
        .await
        .expect("the event read must run")
        .iter()
        .map(|row| row.try_get(0).expect("event_type is text"))
        .collect()
    }

    /// How many rows carry `digest` as their token hash.
    ///
    /// The negative half of "only the hash is stored": a suite proves the token
    /// itself is absent by searching for it, and proves the digest is present
    /// by finding exactly one row under it.
    pub(crate) async fn rows_with_token_hash(&self, digest: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("SELECT count(*) FROM fleet.runners WHERE token_hash = $1")
            .bind(digest)
            .fetch_one(&mut *connection)
            .await
            .expect("the count must run")
            .try_get(0)
            .expect("count answers a bigint")
    }

    /// The lease store over this fixture's database.
    ///
    /// Built per call rather than held: `Leases` is a handle over the same
    /// `Arc`-backed pool, so a second one costs a clone and shares the
    /// connections — which is also what makes it honest for a test to hold one
    /// beside [`Fixtures::runners`].
    pub(crate) fn leases(&self) -> Leases {
        Leases::new(self.database.clone(), self.queue().clone(), Entropy::new())
    }

    /// The same store, over a queue that will not answer.
    ///
    /// Live Postgres, dead Redis — which is the shape a partial outage actually
    /// takes and the only one that proves the publish path degrades rather than
    /// failing the verb. A fixture that took BOTH away would refuse at the
    /// first row read and never reach the publish at all.
    pub(crate) fn leases_with_dead_queue(&self) -> Leases {
        Leases::new(
            self.database.clone(),
            crate::queue::unreachable(),
            Entropy::new(),
        )
    }

    /// The approval-gate store over this fixture's database.
    ///
    /// Built per call for the same reason [`Fixtures::leases`] is: a handle
    /// over the same `Arc`-backed pool.
    pub(crate) fn gates(&self) -> Gates {
        Gates::new(self.database.clone(), self.queue().clone(), Entropy::new())
    }

    /// The queue handle, for the suites that seed a stream or a readiness mark.
    pub(crate) fn queue(&self) -> &Redis {
        self.queue
            .as_ref()
            .expect("this fixture has no queue — build it with Fixtures::create_with_queue")
    }

    /// Drops the database. Best-effort: a leaked test database is noise in a
    /// disposable environment, not a failure worth masking the real one with.
    pub(crate) async fn cleanup(self) {
        let Self {
            base_url,
            name,
            database,
            queue,
            runners,
        } = self;
        // Our own handles go first, so the drop is not racing connections this
        // test still holds. `WITH (FORCE)` would evict them anyway; closing is
        // the difference between a clean teardown and one that relies on it.
        drop(runners);
        // The queue handle goes with the destructuring: it is a clone over a
        // connection the other suites still hold, so there is nothing to close.
        drop((database, queue));

        let statement = AssertSqlSafe(format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"));
        let Ok(pool) = sqlx::PgPool::connect(&base_url).await else {
            return;
        };
        if let Ok(mut connection) = pool.acquire().await {
            let _dropped = sqlx::query(statement).execute(&mut *connection).await;
        }
        pool.close().await;
    }
}

/// The connection URL for one named database on the lane's server.
fn database_url(base_url: &str, name: &str) -> String {
    let (prefix, tail) = base_url
        .rsplit_once('/')
        .expect("a Postgres URL carries a database path");
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    if query.is_empty() {
        format!("{prefix}/{name}")
    } else {
        format!("{prefix}/{name}?{query}")
    }
}

/// Opens a pool for one role against `url`.
async fn open(url: &str, role: DbRole) -> Db {
    let env = MapEnv::from_pairs(DbRole::ALL.iter().map(|each| (each.url_knob(), url)));
    Db::connect(&PoolConfig::resolve(&env, role).expect("the test URL resolves"))
        .await
        .expect("the test database must accept a connection")
}

/// Runs one statement on the lane's admin database.
async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane's database must be reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement must run");
    drop(connection);
    pool.close().await;
}

/// Installs a subscriber so event macros actually run.
///
/// `tracing::warn!` asks whether its callsite is enabled BEFORE evaluating the
/// fields inside it, so with no subscriber a diagnostic's fields never run —
/// the failure path executes and the line reporting it does not. Output goes to
/// a sink; the point is evaluation, not reading.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _previous = tracing::subscriber::set_global_default(subscriber);
    });
}
