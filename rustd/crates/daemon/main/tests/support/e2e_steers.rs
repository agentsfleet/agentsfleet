//! What the fleet-to-fleet suites share: a booted daemon, more fleets in the
//! scenario's workspace its runner is assigned, and the steer and fleets
//! verbs a running fleet calls.
//!
//! Built on [`Speaking`], because a steer's reply destination is copied from
//! a Slack thread the sender was asked in, and the thread's poster must be a
//! loopback.
#![allow(
    dead_code,
    reason = "test support: shared by several suites, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::e2e::{Scenario, unique_ids};
use crate::e2e_seed::{FLEET_CONFIG_JSON, Seeded, scenario_tag, seed_fleet_as};
use crate::speaking::Speaking;
use crate::wire::{get, json as body_of, post};

/// A copy of a fleet's head revision, stamped with a policy this build does
/// not run (`afd_turns/tests/receipts/locking.rs` writes the same).
const FOREIGN_REVISION: &str = "INSERT INTO fleet.revisions \
    (id, fleet_id, revision, definition, canonical_digest, policy_semantics, runtime_model_ref, created_at) \
    SELECT $2, r.fleet_id, r.revision + 1, r.definition, md5(random()::text), 'foreign-semantics', \
           r.runtime_model_ref, now() \
    FROM fleet.revisions r JOIN fleet.fleets f ON f.head_revision_id = r.id WHERE f.id = $1";

/// A fleet's stored status words, as the seed writes them.
pub(crate) const ACTIVE: &str = "active";
/// See [`ACTIVE`].
pub(crate) const ARCHIVED: &str = "archived";
/// See [`ACTIVE`].
pub(crate) const INSTALLING: &str = "installing";

/// A booted daemon with the scenario's fleet as the sender, and the fleets
/// a suite seeds beside it.
pub(crate) struct Crew {
    pub(crate) speaking: Speaking,
    seeded: Mutex<Vec<Uuid7>>,
}

/// One admission row, as a suite asserts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdmissionRow {
    pub(crate) id: Uuid7,
    pub(crate) producer: String,
    pub(crate) actor: String,
    pub(crate) event_type: String,
    pub(crate) request_json: String,
    pub(crate) reply_provider: Option<String>,
    pub(crate) reply_address: Option<String>,
}

/// An [`AdmissionRow`] as the read answers it, column for column.
type Columns = (
    Uuid7,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);

impl Crew {
    pub(crate) async fn boot(supervisor: &mut Supervisor) -> Self {
        Self::boot_with(supervisor, FLEET_CONFIG_JSON).await
    }

    /// [`Crew::boot`], the sender fleet running the setup `config` describes.
    pub(crate) async fn boot_with(supervisor: &mut Supervisor, config: &str) -> Self {
        Self {
            speaking: Speaking::boot(supervisor, config).await,
            seeded: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn run(&self) -> &Scenario {
        &self.speaking.scene
    }

    /// An active fleet named `name` in the scenario's workspace that no
    /// runner is assigned: a steer's target that only has to be written to.
    ///
    /// Unplaced, because the scenario's runner holds one run at a time, and
    /// a target it was assigned would take that slot from the event the test
    /// waits on next.
    pub(crate) async fn sibling(&self, name: &str) -> Uuid7 {
        self.sibling_as(name, ACTIVE).await
    }

    /// [`Crew::sibling`] in `status`.
    pub(crate) async fn sibling_as(&self, name: &str, status: &str) -> Uuid7 {
        let (fleet, _workspace, _tenant) = unique_ids();
        let tag = scenario_tag(&fleet);
        self.seed_sibling(&fleet, name, status, &tag).await;
        fleet
    }

    /// An active fleet named `name` in the scenario's workspace whose work the
    /// scenario's runner is assigned: a target the test runs.
    pub(crate) async fn running_sibling(&self, name: &str) -> Uuid7 {
        let (fleet, _workspace, _tenant) = unique_ids();
        let tag = scenario_tag(&self.run().fleet);
        self.seed_sibling(&fleet, name, ACTIVE, &tag).await;
        fleet
    }

    async fn seed_sibling(&self, fleet: &Uuid7, name: &str, status: &str, tag: &str) {
        let scene = self.run();
        let seed = Seeded { name, status, tag };
        let now = af_core::clock::now();
        seed_fleet_as(
            &scene.booted,
            fleet,
            &scene.workspace,
            &scene.tenant,
            FLEET_CONFIG_JSON,
            now,
            seed,
        )
        .await;
        self.seeded.lock().await.push(fleet.clone());
    }

    /// Moves `fleet`'s setup to a new head revision stamped with a policy this
    /// build does not run, as a setup that needs review is. A new row, because
    /// `fleet.revisions` is append-only.
    pub(crate) async fn stale_setup(&self, fleet: &Uuid7) {
        let revision = afd_db::test_util::mint_id();
        let mut connection = self
            .run()
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query(FOREIGN_REVISION)
            .bind(fleet)
            .bind(&revision)
            .execute(&mut *connection)
            .await
            .expect("the foreign revision inserts");
        sqlx::query("UPDATE fleet.fleets SET head_revision_id = $2 WHERE id = $1")
            .bind(fleet)
            .bind(&revision)
            .execute(&mut *connection)
            .await
            .expect("the head moves");
    }

    /// Lets the scenario's runner hold `workers` runs at once: its row's
    /// worker count, which the run insert counts against, and the session's
    /// own capacity.
    pub(crate) async fn workers(&self, workers: usize) {
        let scene = self.run();
        let count = i32::try_from(workers).expect("a small worker count");
        let mut connection = scene.booted.database.acquire().await.expect("a connection");
        sqlx::query("UPDATE runner.runners SET worker_count = $2 WHERE id = $1")
            .bind(&scene.runner_id)
            .bind(count)
            .execute(&mut *connection)
            .await
            .expect("the worker count moves");
        scene.native_session().await.capacity(workers);
    }

    /// An active fleet in a workspace of its own: one the scenario's fleet
    /// must never reach.
    pub(crate) async fn foreign(&self, name: &str) -> Uuid7 {
        let scene = self.run();
        let (fleet, workspace, tenant) = unique_ids();
        let tag = scenario_tag(&fleet);
        let seed = Seeded {
            name,
            status: ACTIVE,
            tag: &tag,
        };
        let now = af_core::clock::now();
        seed_fleet_as(
            &scene.booted,
            &fleet,
            &workspace,
            &tenant,
            FLEET_CONFIG_JSON,
            now,
            seed,
        )
        .await;
        self.seeded.lock().await.push(fleet.clone());
        fleet
    }

    /// Posts a steer from the run `(run_id, fence)` to `target`.
    pub(crate) async fn steer(
        &self,
        (run_id, fence): (&str, u64),
        target: &Uuid7,
        text: &str,
    ) -> (u16, Value) {
        let path = format!("/v1/runners/me/runs/{run_id}/steers");
        let body = json!({"fencing_token": fence, "fleet_id": target.as_str(), "text": text});
        self.post(&path, &body).await
    }

    /// Posts `body` to `path` as the scenario's runner.
    pub(crate) async fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        let response = post(&self.speaking.http, self.run(), path, body).await;
        let status = response.status().as_u16();
        (status, body_of(response).await)
    }

    /// Reads `path` as the scenario's runner.
    pub(crate) async fn read(&self, path: &str) -> (u16, Value) {
        let response = get(&self.speaking.http, self.run(), path).await;
        let status = response.status().as_u16();
        (status, body_of(response).await)
    }

    /// Every admission on `fleet`, oldest first.
    pub(crate) async fn admissions(&self, fleet: &Uuid7) -> Vec<AdmissionRow> {
        let mut connection = self
            .run()
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        let rows: Vec<Columns> = sqlx::query_as(
            "SELECT id, producer, actor, kind, request_json::text, reply_provider, reply_address \
             FROM thread.turns WHERE thread_id = $1 AND producer IS NOT NULL ORDER BY sequence",
        )
        .bind(fleet)
        .fetch_all(&mut *connection)
        .await
        .expect("the admissions read");
        rows.into_iter()
            .map(
                |(id, producer, actor, event_type, request_json, reply_provider, reply_address)| {
                    AdmissionRow {
                        id,
                        producer,
                        actor,
                        event_type,
                        request_json,
                        reply_provider,
                        reply_address,
                    }
                },
            )
            .collect()
    }

    /// Runs `statement` with `fleet` bound as `$1`, for a state a suite sets
    /// directly: a status, a stale setup.
    pub(crate) async fn alter(&self, statement: &'static str, fleet: &Uuid7) {
        let mut connection = self
            .run()
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query(statement)
            .bind(fleet)
            .execute(&mut *connection)
            .await
            .expect("the fixture statement runs");
    }

    /// Writes a processed chat event on `fleet` whose stored answer is
    /// `answer`, and answers its id.
    pub(crate) async fn answered_event(&self, fleet: &Uuid7, answer: &str) -> Uuid7 {
        let event = afd_db::test_util::mint_id();
        let mut connection = self
            .run()
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query(
            "INSERT INTO thread.turns (thread_id, workspace_id, id, actor, kind, status, \
             request_json, answer_text, opened_at, settled_at, created_at, updated_at) \
             VALUES ($1, $2, $3, 'steer:fixture', 'chat', 'processed', '{\"message\":\"asked\"}'::jsonb, \
             $4, now(), now(), now(), now())",
        )
        .bind(fleet)
        .bind(&self.run().workspace)
        .bind(&event)
        .bind(answer)
        .execute(&mut *connection)
        .await
        .expect("the answered event inserts");
        event
    }

    /// Retires every fleet this crew seeded, then the scenario.
    pub(crate) async fn finish(self, supervisor: Supervisor) {
        for fleet in self.seeded.lock().await.iter() {
            crate::e2e_retire::retire_fleet(&self.speaking.scene.booted, fleet).await;
        }
        self.speaking.finish(supervisor).await;
    }
}
