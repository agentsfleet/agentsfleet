//! What the fleet-to-fleet suites share: a booted daemon, more fleets in the
//! scenario's workspace its runner is offered, and the steer and fleets
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
/// not run (`afd_admission/tests/receipts/locking.rs` writes the same).
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
        &self.speaking.run
    }

    /// An active fleet named `name` in the scenario's workspace that no
    /// runner is offered: a steer's target that only has to be written to.
    ///
    /// Unplaced, because the scenario's runner holds one lease at a time, and
    /// a target it was offered would take that slot from the event the test
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
    /// scenario's runner is offered: a target the test runs.
    pub(crate) async fn running_sibling(&self, name: &str) -> Uuid7 {
        let (fleet, _workspace, _tenant) = unique_ids();
        let tag = scenario_tag(&self.run().fleet);
        self.seed_sibling(&fleet, name, ACTIVE, &tag).await;
        fleet
    }

    async fn seed_sibling(&self, fleet: &Uuid7, name: &str, status: &str, tag: &str) {
        let run = self.run();
        let seed = Seeded { name, status, tag };
        let now = af_core::clock::now();
        seed_fleet_as(
            &run.booted,
            fleet,
            &run.workspace,
            &run.tenant,
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

    /// Lets the scenario's runner hold `workers` leases at once: its row's
    /// worker count, which the lease insert counts against, and the session's
    /// own capacity.
    pub(crate) async fn workers(&self, workers: usize) {
        let run = self.run();
        let count = i32::try_from(workers).expect("a small worker count");
        let mut connection = run.booted.database.acquire().await.expect("a connection");
        sqlx::query("UPDATE runner.runners SET worker_count = $2 WHERE id = $1")
            .bind(&run.runner_id)
            .bind(count)
            .execute(&mut *connection)
            .await
            .expect("the worker count moves");
        run.native_session().await.capacity(workers);
    }

    /// An active fleet in a workspace of its own: one the scenario's fleet
    /// must never reach.
    pub(crate) async fn foreign(&self, name: &str) -> Uuid7 {
        let run = self.run();
        let (fleet, workspace, tenant) = unique_ids();
        let tag = scenario_tag(&fleet);
        let seed = Seeded {
            name,
            status: ACTIVE,
            tag: &tag,
        };
        let now = af_core::clock::now();
        seed_fleet_as(
            &run.booted,
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

    /// Posts a steer from the lease `(lease_id, fence)` to `target`.
    pub(crate) async fn steer(
        &self,
        (lease_id, fence): (&str, u64),
        target: &Uuid7,
        text: &str,
    ) -> (u16, Value) {
        let path = format!("/v1/runners/me/leases/{lease_id}/steers");
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
            "SELECT id, producer, actor, event_type, request_json, reply_provider, reply_address \
             FROM fleet.admissions WHERE fleet_id = $1 ORDER BY sequence",
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

    /// Retires every fleet this crew seeded, then the scenario.
    pub(crate) async fn finish(self, supervisor: Supervisor) {
        for fleet in self.seeded.lock().await.iter() {
            crate::e2e_retire::retire_fleet(&self.speaking.run.booted, fleet).await;
        }
        self.speaking.finish(supervisor).await;
    }
}

/// What one offer carried: its lease, its fence, its event and fleet, the
/// message the model reads, and the earlier turns' messages, oldest first.
pub(crate) struct Offered {
    pub(crate) lease_id: String,
    pub(crate) fence: u64,
    pub(crate) event_id: Uuid7,
    pub(crate) fleet_id: Uuid7,
    pub(crate) message: String,
    pub(crate) history: Vec<String>,
}

/// The next fleet offer the scenario's runner is pushed, whichever event it
/// carries.
pub(crate) async fn next_offer(run: &Scenario) -> Offered {
    let client = run.native_session().await;
    let reading = async {
        loop {
            let frame = client.offer().await.expect("the native owner remains live");
            let message: af_wire::protocol::Down<'_> = frame.decode().expect("the offer decodes");
            let af_wire::protocol::Down::Offer {
                lease: af_wire::lease::TaggedLease::Fleet(lease),
            } = message
            else {
                continue;
            };
            return Offered {
                lease_id: lease.lease_id.to_string(),
                fence: lease.fencing_token,
                event_id: Uuid7::parse(&lease.event.event_id).expect("an offered event id"),
                fleet_id: Uuid7::parse(&lease.event.fleet_id).expect("an offered fleet id"),
                message: af_wire::event::message_of(&lease.event.request_json).into_owned(),
                history: lease
                    .history
                    .iter()
                    .map(|turn| turn.message.to_string())
                    .collect(),
            };
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), reading)
        .await
        .expect("an event is offered")
}

/// The offer of `event_id`, when it is the only work the runner is waiting on:
/// an offer of any other event fails the test, since it would hold the slot.
pub(crate) async fn offered(run: &Scenario, event_id: &Uuid7) -> Offered {
    let offer = next_offer(run).await;
    assert_eq!(&offer.event_id, event_id, "another event was offered first");
    offer
}

/// Settles `offer` with the harness's processed report.
pub(crate) async fn settle(run: &Scenario, offer: &Offered) {
    let body = crate::wire::report_body(&offer.lease_id, &offer.event_id, offer.fence);
    let reported = crate::wire::report(run, &body).await;
    assert!(reported.is_ok(), "the run settles: {reported:?}");
}

/// The event id a steer was answered with.
pub(crate) fn steered(answer: &Value) -> Uuid7 {
    Uuid7::parse(
        answer["event_id"]
            .as_str()
            .expect("a steer answers its event"),
    )
    .expect("a canonical event id")
}
