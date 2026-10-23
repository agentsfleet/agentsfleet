//! The lane's database, and the runner store over it.
//!
//! Ordinary row tests share the migrated lane database and mint their own ids.
//! Tests that corrupt globally listed rows use `Fixtures::create_isolated`:
//! namespacing cannot keep a malformed row out of an unfiltered operator read.
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
use afd_dragonfly::Dragonfly;
use afd_fleet::lease::Leases;
use afd_gate::gate::Gates;
use afd_runner::Runners;
use jiff::Timestamp;
use sqlx::AssertSqlSafe;

/// The model catalogue's table, and the name it hides under while offline.
const CATALOGUE_TABLE: &str = "platform_models";
const CATALOGUE_OFFLINE: &str = "platform_models_offline";

/// A handle on the lane's database, and the runner store over it.
pub(crate) struct Fixtures {
    lane: TestDatabase,
    pub(crate) database: Db,
    queue: Option<Dragonfly>,
    runners: Runners,
    enrolled: Timestamp,
}

impl Fixtures {
    /// Opens the api-role pool against the database the lane already migrated.
    ///
    /// No Dragonfly. Most suites here assert on ROWS and never read the queue, and
    /// connecting one anyway is not free: each is a TLS handshake, and a suite
    /// running its tests in parallel opened enough at once to time out against
    /// a healthy server. Use [`Fixtures::create_with_queue`] where the queue is
    /// actually read.
    pub(crate) async fn create() -> Self {
        Self::from_lane(TestDatabase::shared()).await
    }

    /// Migrate a private database before injecting corruption visible to global reads.
    pub(crate) async fn create_isolated() -> Self {
        let lane = TestDatabase::create().await;
        let migrator = lane.open(DbRole::Migrator, &[]).await;
        afd_db::Migrator::new()
            .run(&migrator)
            .await
            .expect("the isolated fixture has the production schema");
        migrator.close().await;
        Self::from_lane(lane).await
    }

    async fn from_lane(lane: TestDatabase) -> Self {
        let database = lane.open(DbRole::Api, &[]).await;
        Self {
            runners: Runners::new(database.clone(), Entropy::new()),
            database,
            queue: None,
            lane,
            enrolled: afd_core::clock::now(),
        }
    }

    /// The instant this test's runners enrol at: the wall clock when the
    /// fixture opened, then fixed, so a row's stamps are asserted rather than
    /// observed.
    ///
    /// The wall clock rather than a constant, because issuing and renewing a
    /// lease and counting a host's live leases read the DATABASE's clock. A
    /// lease stamped from another instant has either lapsed before it was
    /// written or outlives every sweep, and the meter would charge the distance
    /// between the two clocks. A suite that needs a lease to lapse ages its row,
    /// not its clock.
    ///
    /// Per fixture, never per binary: a lease lives `LEASE_TTL` (30 s) past
    /// this instant, so one shared instant expired every lease a test took
    /// after the suite's first 30 s.
    pub(crate) const fn enrolled_at(&self) -> Timestamp {
        self.enrolled
    }

    /// [`Fixtures::enrolled_at`] moved forward by `by`: a later instant a test
    /// hands the code under test, such as a report sent after the run.
    pub(crate) fn after(&self, by: std::time::Duration) -> Timestamp {
        afd_core::clock::after(self.enrolled, by)
    }

    /// The same as [`Fixtures::create_isolated`], plus the lane's Dragonfly.
    ///
    /// The pairing a deployment-WIDE assertion needs. A budget counted across
    /// every unsettled row in the database cannot be proven to clear on the
    /// shared lane, where a sibling suite's deferred row keeps the count up;
    /// a private database makes this test the only writer of the number it
    /// asserts on. Dragonfly stays shared, which is harmless — its keys are
    /// namespaced by the fleet ids this test mints.
    pub(crate) async fn create_isolated_with_queue() -> Self {
        let mut fixtures = Self::create_isolated().await;
        fixtures.queue = Some(crate::queue::connect().await);
        fixtures
    }

    /// The same, plus the lane's Dragonfly.
    ///
    /// Shared rather than per-test: Dragonfly has no database-per-test equivalent,
    /// and the keys these suites touch are namespaced by the fleet ids each
    /// test declares for itself.
    pub(crate) async fn create_with_queue() -> Self {
        let mut fixtures = Self::create().await;
        fixtures.queue = Some(crate::queue::connect().await);
        fixtures
    }

    /// A further pool for `role` on this fixture's database — the shared lane's
    /// or this test's private one — as a second daemon replica would hold.
    pub(crate) async fn open(&self, role: DbRole) -> Db {
        self.lane.open(role, &[]).await
    }

    /// The store under test.
    pub(crate) const fn runners(&self) -> &Runners {
        &self.runners
    }

    /// The event types written for a runner, oldest first.
    pub(crate) async fn events(&self, runner: &str) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar(
            "SELECT event_type FROM runner.events \
             WHERE runner_id = $1::uuid ORDER BY created_at, id",
        )
        .bind(runner)
        .fetch_all(&mut *connection)
        .await
        .expect("the event read must run")
    }

    /// Operator subjects recorded on administrative events, oldest first.
    pub(crate) async fn admin_event_actors(&self, runner: &str) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar(
            "SELECT metadata ->> 'actor_id' FROM runner.events \
             WHERE runner_id = $1::uuid AND metadata ? 'actor_id' ORDER BY created_at, id",
        )
        .bind(runner)
        .fetch_all(&mut *connection)
        .await
        .expect("the actor metadata read must run")
    }

    /// How many rows carry `digest` as their token hash.
    ///
    /// The negative half of "only the hash is stored": a suite proves the token
    /// itself is absent by searching for it, and proves the digest is present
    /// by finding exactly one row under it.
    pub(crate) async fn rows_with_token_hash(&self, digest: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT count(*) FROM runner.runners WHERE token_hash = $1")
            .bind(digest)
            .fetch_one(&mut *connection)
            .await
            .expect("the count must run")
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
    /// Live Postgres, dead Dragonfly — which is the shape a partial outage actually
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

    /// Dragonfly for tests reading activity frames or outbound delivery.
    pub(crate) fn queue(&self) -> &Dragonfly {
        self.queue
            .as_ref()
            .expect("this fixture has no queue — build it with Fixtures::create_with_queue")
    }

    /// Takes the model catalogue away from the api role, or gives it back.
    ///
    /// A renamed table, so the rate read fails as a datastore fault rather than
    /// answering "no such model", which is a priced miss. Private databases
    /// only: every test on the shared lane reads this table.
    pub(crate) async fn set_catalogue_readable(&self, readable: bool) {
        assert!(
            self.lane.database_name().is_some(),
            "only a private database may lose its catalogue: build it with create_isolated"
        );
        let (from, to) = if readable {
            (CATALOGUE_OFFLINE, CATALOGUE_TABLE)
        } else {
            (CATALOGUE_TABLE, CATALOGUE_OFFLINE)
        };
        let migrator = self.lane.open(DbRole::Migrator, &[]).await;
        let mut connection = migrator.acquire().await.expect("a migrator connection");
        sqlx::query(AssertSqlSafe(format!(
            "ALTER TABLE model.{from} RENAME TO {to}"
        )))
        .execute(&mut *connection)
        .await
        .expect("the migrator owns the catalogue table");
        drop(connection);
        migrator.close().await;
    }

    /// Releases this test's handles and drops its database when privately owned.
    pub(crate) async fn cleanup(self) {
        let Self {
            lane,
            database,
            queue,
            runners,
            ..
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
