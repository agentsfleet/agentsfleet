//! The lane's database, and the runner store over it.
//!
//! # One database, and what keeps the tests apart in it
//!
//! Every assertion here is about a ROW — what enrolment wrote, what a beat
//! moved — and every one of those rows is keyed by a runner or fleet identifier
//! the test minted for itself. That is the isolation. A database per test was
//! belt over braces and cost forty-seven migrations apiece to provide.
//!
//! It is also the isolation the queue half has always used, and says so two
//! doc comments below: Redis has no database-per-test equivalent, so keys are
//! namespaced by the ids each test declares. Postgres now works the same way.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which is where the four
//! near-identical copies of this were always headed — one of the deletions that
//! module predicted.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
// Two test binaries include this module by `#[path]`, so each compiles its own
// copy and uses a subset of it — what the row suite never calls is not dead
// code, it is the beat suite's half.
#![allow(
    dead_code,
    reason = "test support: shared by two test binaries, each using a subset"
)]

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_fleet::lease::Leases;
use afd_gate::gate::Gates;
use afd_redis::Redis;
use afd_runner::Runners;
use sqlx::{AssertSqlSafe, Row as _};

/// A handle on the lane's database, and the runner store over it.
pub(crate) struct Fixtures {
    lane: TestDatabase,
    pub(crate) database: Db,
    queue: Option<Redis>,
    runners: Runners,
}

impl Fixtures {
    /// Opens the api-role pool against the database the lane already migrated.
    ///
    /// No Redis. Most suites here assert on ROWS and never read the queue, and
    /// connecting one anyway is not free: each is a TLS handshake, and a suite
    /// running its tests in parallel opened enough at once to time out against
    /// a healthy server. Use [`Fixtures::create_with_queue`] where the queue is
    /// actually read.
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        Self {
            runners: Runners::new(database.clone(), Entropy::new()),
            database,
            queue: None,
            lane,
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

    /// Operator subjects recorded on administrative events, oldest first.
    pub(crate) async fn admin_event_actors(&self, runner: &str) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "SELECT metadata ->> 'actor_id' FROM fleet.runner_events \
             WHERE runner_id = $1::uuid AND metadata ? 'actor_id' ORDER BY created_at, id",
        )
        .bind(runner)
        .fetch_all(&mut *connection)
        .await
        .expect("the actor metadata read must run")
        .iter()
        .map(|row| row.try_get(0).expect("actor_id is text"))
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

    /// Releases this test's handles. A no-op against the shared database, and
    /// called anyway: it is how a test says it is finished.
    pub(crate) async fn cleanup(self) {
        let Self {
            lane,
            database,
            queue,
            runners,
        } = self;
        // Our own handles go first, so nothing is still borrowing a connection
        // when the lane handle is released.
        drop(runners);
        // The queue handle goes with the destructuring: it is a clone over a
        // connection the other suites still hold, so there is nothing to close.
        drop((database, queue));
        lane.cleanup().await;
    }
}

/// Installs a subscriber so event macros actually run.
///
/// Re-exported from [`afd_db::test_util`] rather than kept as a fourth copy:
/// the suites here call it directly, and one implementation is the point of the
/// consolidation.
pub(crate) fn install_subscriber() {
    afd_db::test_util::install_subscriber();
}
