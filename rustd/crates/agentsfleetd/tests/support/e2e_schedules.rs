//! What the runner schedules suites share: a booted daemon pointed at a fake
//! `QStash`, its lease taken, and the rows a case seeds or reads directly.
//!
//! Split from the suites by concern (RULE FLL): each suite file holds one
//! family of cases, and this holds the lease they all act under.
#![allow(
    dead_code,
    reason = "test support: shared by several suites, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_cron::{ACTOR_PREFIX, Source};
use afd_crypto::entropy::Entropy;
use agentsfleetd::preflight::{QSTASH_TOKEN_KNOB, QSTASH_URL_KNOB};
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::Row as _;

use crate::e2e::{Scenario, scenario_with};
use crate::e2e_seed::{FLEET_CONFIG_JSON, seed_fleet};
use crate::tail::lease;
use crate::verbs::{FakeQStash, QSTASH_TOKEN, send};
use crate::wire::{post, report_body};

/// The expression and zone every created schedule here fires on.
pub(crate) const WEEKLY: &str = "0 9 * * 1";
/// See [`WEEKLY`].
pub(crate) const KOLKATA: &str = "Asia/Kolkata";

/// A schedule row as a case seeds it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seeded<'a> {
    pub(crate) source: Source,
    pub(crate) key: &'a str,
    pub(crate) desired_status: &'a str,
    pub(crate) once: bool,
}

impl<'a> Seeded<'a> {
    /// An active, recurring row of `source` under `key`.
    pub(crate) const fn active(source: Source, key: &'a str) -> Self {
        Self {
            source,
            key,
            desired_status: "active",
            once: false,
        }
    }
}

/// A booted daemon pointed at `qstash`, its lease taken.
pub(crate) struct Leased {
    pub(crate) run: Scenario,
    pub(crate) http: reqwest::Client,
    pub(crate) lease_id: String,
    pub(crate) fence: u64,
}

/// The second fleet's name: one name per workspace, and the scenario's own
/// fleet holds [`crate::e2e_seed::FLEET_NAME`].
const OTHER_FLEET_NAME: &str = "e2e-other-fleet";

/// Where a runner reports a lease's end.
const REPORTS: &str = "/v1/runners/me/reports";

impl Leased {
    pub(crate) async fn boot(supervisor: &mut Supervisor, qstash: &FakeQStash) -> Self {
        let extra = [
            (QSTASH_URL_KNOB, qstash.base()),
            (QSTASH_TOKEN_KNOB, QSTASH_TOKEN),
        ];
        let run = scenario_with(supervisor, None, &extra, FLEET_CONFIG_JSON).await;
        let http = reqwest::Client::new();
        let (lease_id, fence) = lease(&http, &run).await;
        Self {
            run,
            http,
            lease_id,
            fence,
        }
    }

    /// `path` under this lease's schedules collection.
    pub(crate) fn path(&self, tail: &str) -> String {
        format!("/v1/runners/me/leases/{}/schedules{tail}", self.lease_id)
    }

    /// The fence as a read's or a delete's query.
    pub(crate) fn fenced(&self, tail: &str) -> String {
        format!("{}?fencing_token={}", self.path(tail), self.fence)
    }

    /// The fence as a write's body.
    pub(crate) fn fence_body(&self) -> Value {
        json!({"fencing_token": self.fence})
    }

    pub(crate) async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> (u16, Value) {
        let response = send(&self.http, &self.run, method, path, body).await;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.expect("the body is readable");
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Creates a schedule under this lease's fence, answering the status and
    /// the body.
    pub(crate) async fn create(&self, once: bool) -> (u16, Value) {
        let body = json!({"fencing_token": self.fence, "cron": WEEKLY, "timezone": KOLKATA,
                          "message": "weekly check", "once": once});
        self.call(Method::POST, &self.path(""), Some(&body)).await
    }

    /// Settles the held lease, so the fleet takes its next event.
    pub(crate) async fn settle(&self) {
        let report = report_body(&self.lease_id, &self.run.event_id, self.fence);
        let reported = post(&self.http, &self.run, REPORTS, &report).await;
        assert_eq!(reported.status().as_u16(), 200, "the held lease settles");
    }

    /// Runs `schedule` now under this lease's fence.
    pub(crate) async fn run_now(&self, schedule: &str) -> (u16, Value) {
        let runs = self.path(&format!("/{schedule}/runs"));
        self.call(Method::POST, &runs, Some(&self.fence_body()))
            .await
    }

    /// How many schedules of `source` the fleet holds, or of any source.
    pub(crate) async fn held(&self, source: Option<Source>) -> i64 {
        let statement = match source {
            Some(_) => {
                "SELECT count(*) FROM core.fleet_schedules WHERE fleet_id = $1::uuid AND source = $2"
            }
            None => {
                "SELECT count(*) FROM core.fleet_schedules WHERE fleet_id = $1::uuid AND $2 = ''"
            }
        };
        let mut connection = self.connection().await;
        sqlx::query(statement)
            .bind(&self.run.fleet)
            .bind(source.map_or("", Source::as_str))
            .fetch_one(&mut *connection)
            .await
            .expect("the count runs")
            .try_get(0)
            .expect("an integer")
    }

    /// Writes a schedule straight into the table, as a person's surface or
    /// an earlier run would have.
    pub(crate) async fn seed(&self, source: Source, key: &str) -> String {
        self.seed_row(&self.run.fleet, Seeded::active(source, key))
            .await
    }

    /// Writes `row` for `fleet` straight into the table.
    pub(crate) async fn seed_row(&self, fleet: &str, row: Seeded<'_>) -> String {
        let now = afd_core::clock::now();
        let id = Entropy::new().uuid7(now).expect("an identifier");
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.fleet_schedules (id, fleet_id, source, source_key, cron_expression, \
             timezone, message, desired_status, sync_status, generation, created_at, updated_at, \
             once) VALUES ($1::uuid, $2::uuid, $3, $4, $5, 'UTC', 'seeded', $6, 'synced', 1, $7, \
             $7, $8)",
        )
        .bind(id.as_str())
        .bind(fleet)
        .bind(row.source.as_str())
        .bind(row.key)
        .bind(WEEKLY)
        .bind(row.desired_status)
        .bind(now.as_millis())
        .bind(row.once)
        .execute(&mut *connection)
        .await
        .expect("the seeded schedule inserts");
        id.as_str().to_owned()
    }

    /// A second fleet in this scenario's workspace, for cross-fleet cases;
    /// [`Self::finish_with`] retires it.
    pub(crate) async fn other_fleet(&self) -> String {
        let now = afd_core::clock::now();
        let fleet = Entropy::new().uuid7(now).expect("an identifier");
        seed_fleet(
            &self.run.booted,
            fleet.as_str(),
            OTHER_FLEET_NAME,
            &self.run.workspace,
            &self.run.tenant,
            FLEET_CONFIG_JSON,
            now,
        )
        .await;
        fleet.as_str().to_owned()
    }

    /// Runs one statement against this scenario's fleet, `$1` its id.
    pub(crate) async fn on_fleet(&self, statement: &'static str) {
        let mut connection = self.connection().await;
        sqlx::query(statement)
            .bind(&self.run.fleet)
            .execute(&mut *connection)
            .await
            .expect("the fixture statement runs");
    }

    /// Moves the fleet's live sequence past this lease's, as a reclaim does.
    pub(crate) async fn supersede(&self) {
        self.on_fleet(
            "UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1 WHERE fleet_id = $1::uuid",
        )
        .await;
    }

    /// Records that a schedule's fire woke the leased event, as a `QStash`
    /// callback's admission would.
    pub(crate) async fn lease_woken_by_schedule(&self) {
        let mut connection = self.connection().await;
        sqlx::query("UPDATE fleet.runner_leases SET actor = $2 WHERE id = $1::uuid")
            .bind(&self.lease_id)
            .bind(format!(
                "{ACTOR_PREFIX}0195b4ba-8d3a-7000-8abc-00000000c401"
            ))
            .execute(&mut *connection)
            .await
            .expect("the lease's actor moves");
    }

    /// Hands `schedule`'s row to another syncer for a minute, or back.
    pub(crate) async fn hold_sync(&self, schedule: &str, held: bool) {
        let now = afd_core::clock::now();
        let token = held.then(|| Entropy::new().uuid7(now).expect("a token"));
        let mut connection = self.connection().await;
        sqlx::query(
            "UPDATE core.fleet_schedules SET sync_token = $2::uuid, sync_lease_until = $3 \
             WHERE id = $1::uuid",
        )
        .bind(schedule)
        .bind(token.as_ref().map(afd_core::id::Uuid7::as_str))
        .bind(held.then(|| now.as_millis() + 60_000))
        .execute(&mut *connection)
        .await
        .expect("the sync lease moves");
    }

    /// How many fires the ledger admitted under `schedule`'s actor.
    pub(crate) async fn admitted(&self, schedule: &str) -> i64 {
        let mut connection = self.connection().await;
        sqlx::query_scalar(
            "SELECT count(*) FROM core.fleet_admissions WHERE workspace_id = $1::uuid AND actor = $2",
        )
        .bind(&self.run.workspace)
        .bind(format!("{ACTOR_PREFIX}{schedule}"))
        .fetch_one(&mut *connection)
        .await
        .expect("the ledger answers")
    }

    /// A pooled connection to the lane's database.
    pub(crate) async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.run
            .booted
            .database
            .acquire()
            .await
            .expect("a connection")
    }

    pub(crate) async fn finish(self, supervisor: Supervisor) {
        supervisor.shutdown().await;
        self.run.cleanup().await;
    }

    /// [`Self::finish`], retiring `other` first.
    pub(crate) async fn finish_with(self, supervisor: Supervisor, other: &str) {
        crate::e2e_retire::retire_fleet(&self.run.booted, other).await;
        self.finish(supervisor).await;
    }
}
