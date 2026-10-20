//! The runner plane's request paths against the lane's migrated Postgres: an
//! enrolment, its beats, an operator's transitions and edits, and the reads
//! that answer them.
//!
//! Each test enrols runners of its own and asserts only on them, so the suite
//! runs beside every other in the lane.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

mod faults;
mod lifecycle;
mod operator;
#[path = "../../../afd_authoring/tests/support/refusal.rs"]
mod refusal;

use std::borrow::Cow;

use afd_api_wire::admin::RunnerEventItem;
use afd_core::clock;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_runner::{RunnerEventFilter, Runners};
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};

/// The page size every history read here asks for: more than any test writes.
const HISTORY_PAGE: u32 = 20;

/// An enrolment no capability report is needed for, so its verdict is healthy
/// and a beat with no report writes no verdict.
fn enrolment() -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Borrowed("host-01.plane.test"),
        assigned_policy: AssignedPolicy {
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        labels: vec![Cow::Borrowed("plane")],
    }
}

/// The lane's database, a store over it, and the runners a test enrolled.
struct Fixture {
    lane: TestDatabase,
    database: Db,
    /// The lane owner's pool, for the rows a test stages and removes around
    /// the store; the store itself runs on [`Self::database`], the API login.
    owner: Db,
    runners: Runners,
    enrolled: Vec<Uuid7>,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        Self {
            runners: Runners::new(database.clone(), Entropy::new()),
            database,
            owner: lane.open(DbRole::Migrator, &[]).await,
            enrolled: Vec::new(),
            lane,
        }
    }

    /// Enrols one runner, remembered for cleanup.
    async fn enrol(&mut self) -> Uuid7 {
        let enrolled = self
            .runners
            .register(&enrolment(), clock::now())
            .await
            .expect("the enrolment lands");
        self.enrolled.push(enrolled.runner_id.clone());
        enrolled.runner_id
    }

    /// The runner's whole history, newest first.
    async fn history(&self, runner: &Uuid7) -> Vec<RunnerEventItem<'static>> {
        let unfiltered = RunnerEventFilter::new(Vec::new(), None, None).expect("an open filter");
        self.runners
            .runner_events(runner, &unfiltered, None, HISTORY_PAGE)
            .await
            .expect("the history reads")
            .into_items()
    }

    async fn cleanup(self) {
        let mut connection = self.owner.acquire().await.expect("an owner connection");
        let ids: Vec<&str> = self.enrolled.iter().map(Uuid7::as_str).collect();
        sqlx::query("DELETE FROM runner.runners WHERE id = ANY(($1::text[])::uuid[])")
            .bind(&ids)
            .execute(&mut *connection)
            .await
            .expect("the enrolled runners clean up");
        drop(connection);
        drop(self.runners);
        self.database.close().await;
        self.owner.close().await;
        self.lane.cleanup().await;
    }
}
