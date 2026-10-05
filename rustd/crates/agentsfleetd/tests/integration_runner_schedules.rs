//! §1 — a running fleet keeps its own schedules on the daemon's plane.
//!
//! Each case boots the daemon against the lane's Postgres and Dragonfly, with
//! `QSTASH_URL` pointed at a fake scheduler that records what it is told, so
//! "reconciled to QStash" is a call this suite can count rather than a sync
//! state it infers. The fleet is always the lease's: no body here names one.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just built"
)]

use afd_cron::{ACTOR_PREFIX, Source};
use afd_crypto::entropy::Entropy;
use agentsfleetd::preflight::{QSTASH_TOKEN_KNOB, QSTASH_URL_KNOB};
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::Row as _;

use crate::e2e::{Scenario, scenario_with};
use crate::e2e_seed::FLEET_CONFIG_JSON;
use crate::tail::lease;
use crate::verbs::{FakeQStash, QSTASH_TOKEN, Told, send};

/// The expression and zone every created schedule here fires on.
const WEEKLY: &str = "0 9 * * 1";
/// See [`WEEKLY`].
const KOLKATA: &str = "Asia/Kolkata";

/// A booted daemon pointed at `qstash`, its lease taken.
struct Leased {
    run: Scenario,
    http: reqwest::Client,
    lease_id: String,
    fence: u64,
}

impl Leased {
    async fn boot(supervisor: &mut Supervisor, qstash: &FakeQStash) -> Self {
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
    fn path(&self, tail: &str) -> String {
        format!("/v1/runners/me/leases/{}/schedules{tail}", self.lease_id)
    }

    /// The fence as a read's or a delete's query.
    fn fenced(&self, tail: &str) -> String {
        format!("{}?fencing_token={}", self.path(tail), self.fence)
    }

    async fn call(&self, method: Method, path: &str, body: Option<&Value>) -> (u16, Value) {
        let response = send(&self.http, &self.run, method, path, body).await;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.expect("the body is readable");
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Creates a schedule under this lease's fence, answering the status and
    /// the body.
    async fn create(&self, once: bool) -> (u16, Value) {
        let body = json!({"fencing_token": self.fence, "cron": WEEKLY, "timezone": KOLKATA,
                          "message": "weekly check", "once": once});
        self.call(Method::POST, &self.path(""), Some(&body)).await
    }

    /// How many schedules of `source` the fleet holds.
    async fn held(&self, source: Option<Source>) -> i64 {
        let mut connection = self
            .run
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        let statement = match source {
            Some(_) => {
                "SELECT count(*) FROM core.fleet_schedules WHERE fleet_id = $1::uuid AND source = $2"
            }
            None => {
                "SELECT count(*) FROM core.fleet_schedules WHERE fleet_id = $1::uuid AND $2 = ''"
            }
        };
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
    async fn seed(&self, source: Source, key: &str) -> String {
        let now = afd_core::clock::now();
        let id = Entropy::new().uuid7(now).expect("an identifier");
        let mut connection = self
            .run
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query(
            "INSERT INTO core.fleet_schedules (id, fleet_id, source, source_key, cron_expression, \
             timezone, message, desired_status, sync_status, generation, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, 'UTC', 'seeded', 'active', 'synced', 1, $6, $6)",
        )
        .bind(id.as_str())
        .bind(&self.run.fleet)
        .bind(source.as_str())
        .bind(key)
        .bind(WEEKLY)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .expect("the seeded schedule inserts");
        id.as_str().to_owned()
    }

    /// Moves the fleet's live sequence past this lease's, as a reclaim does.
    async fn supersede(&self) {
        let mut connection = self
            .run
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query(
            "UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1 WHERE fleet_id = $1::uuid",
        )
        .bind(&self.run.fleet)
        .execute(&mut *connection)
        .await
        .expect("the sequence moves");
    }

    async fn finish(self, supervisor: Supervisor) {
        supervisor.shutdown().await;
        self.run.cleanup().await;
    }
}

/// Dimension 1.1. A valid create stores a `fleet`-sourced row, registers it
/// once, and the fleet's own list returns it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fleet_creates_its_own_schedule() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;

    let (status, created) = leased.create(false).await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["source"], "fleet");
    assert_eq!(created["once"], false);
    assert_eq!(
        created["sync"], "synced",
        "the fake scheduler registered it"
    );
    assert_eq!(created["timezone"], KOLKATA);
    assert_eq!(qstash.told(), [Told::Upsert(WEEKLY.to_owned())]);
    assert_eq!(leased.held(Some(Source::Fleet)).await, 1);

    let (status, listed) = leased.call(Method::GET, &leased.fenced(""), None).await;
    assert_eq!(status, 200, "{listed}");
    let schedules = listed["schedules"].as_array().expect("a list");
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0]["schedule_id"], created["schedule_id"]);
    assert_eq!(schedules[0]["source"], "fleet");
    leased.finish(supervisor).await;
}

/// Dimension 1.2. A holder the fleet has moved past stores nothing and reads
/// nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_stale_fence_schedule_refused() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    leased.supersede().await;

    let (status, refused) = leased.create(false).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error_code"], "UZ-RUN-005");
    assert_eq!(leased.held(None).await, 0);
    assert_eq!(qstash.upserts(), 0, "nothing reached the scheduler");
    let (status, _) = leased.call(Method::GET, &leased.fenced(""), None).await;
    assert_eq!(status, 409, "a superseded holder reads nothing either");
    leased.finish(supervisor).await;
}

/// Dimension 1.3. A fleet holding its sixteen refuses the seventeenth, and
/// the scheduler is never asked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_schedule_cap_refuses_with_code() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    for slot in 0..afd_cron::FLEET_SCHEDULES_MAX {
        leased.seed(Source::Fleet, &format!("seeded-{slot}")).await;
    }

    let (status, refused) = leased.create(false).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error_code"], "UZ-SCHED-009");
    assert_eq!(qstash.upserts(), 0);
    let held = i64::try_from(afd_cron::FLEET_SCHEDULES_MAX).expect("a small cap");
    assert_eq!(leased.held(Some(Source::Fleet)).await, held);
    leased.finish(supervisor).await;
}

/// Dimension 1.4. A person's schedule is the fleet's to read, not to change or
/// delete.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fleet_cannot_touch_human_schedules() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    for source in [Source::Api, Source::Trigger] {
        let schedule = leased
            .seed(source, &format!("person-{}", source.as_str()))
            .await;
        let member = format!("/{schedule}");
        let patch = json!({"fencing_token": leased.fence, "message": "taken over"});
        let (status, refused) = leased
            .call(Method::PATCH, &leased.path(&member), Some(&patch))
            .await;
        assert_eq!(
            (status, &refused["error_code"]),
            (403, &json!("UZ-SCHED-010")),
            "{refused}"
        );
        let (status, refused) = leased
            .call(Method::DELETE, &leased.fenced(&member), None)
            .await;
        assert_eq!(
            (status, &refused["error_code"]),
            (403, &json!("UZ-SCHED-010")),
            "{refused}"
        );
    }
    let (_status, listed) = leased.call(Method::GET, &leased.fenced(""), None).await;
    let messages: Vec<&Value> = listed["schedules"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|schedule| &schedule["message"])
        .collect();
    assert_eq!(
        messages,
        [&json!("seeded"), &json!("seeded")],
        "both rows unchanged"
    );
    assert!(qstash.told().is_empty());
    leased.finish(supervisor).await;
}

/// Dimension 1.6. Running now admits one event under the schedule's actor, and
/// a retried post answers the same run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_schedule_run_now_admits_one_event() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let (_status, created) = leased.create(false).await;
    let schedule = created["schedule_id"].as_str().expect("an id").to_owned();
    let runs = leased.path(&format!("/{schedule}/runs"));
    let body = json!({"fencing_token": leased.fence});

    let (status, first) = leased.call(Method::POST, &runs, Some(&body)).await;
    assert_eq!(status, 201, "{first}");
    let (status, again) = leased.call(Method::POST, &runs, Some(&body)).await;
    assert_eq!(status, 201, "{again}");
    assert_eq!(
        first["event_id"], again["event_id"],
        "one lease's retry is one run"
    );

    let mut connection = leased
        .run
        .booted
        .database
        .acquire()
        .await
        .expect("a connection");
    let row = sqlx::query(
        "SELECT count(*), min(event_type) FROM core.fleet_admissions \
         WHERE fleet_id = $1::uuid AND actor = $2",
    )
    .bind(&leased.run.fleet)
    .bind(format!("{ACTOR_PREFIX}{schedule}"))
    .fetch_one(&mut *connection)
    .await
    .expect("the ledger answers");
    let admitted: i64 = row.try_get(0).expect("a count");
    let event_type: String = row.try_get(1).expect("a type");
    assert_eq!((admitted, event_type.as_str()), (1, "cron"));
    drop(connection);
    leased.finish(supervisor).await;
}

/// Dimension 1.7. A schedule's runs are its events, newest first, paged.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_schedule_runs_lists_events() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let schedule = leased.seed(Source::Fleet, "runs-fixture").await;
    let actor = format!("{ACTOR_PREFIX}{schedule}");
    for (event, at) in [
        ("1700000000001-0", 1),
        ("1700000000002-0", 2),
        ("1700000000003-0", 3),
    ] {
        record_event(&leased.run, event, &actor, at).await;
    }
    // Another schedule's run, which this list must not carry.
    record_event(&leased.run, "1700000000004-0", "cron:someone-else", 4).await;

    let first = format!("{}&limit=2", leased.fenced(&format!("/{schedule}/runs")));
    let (status, page) = leased.call(Method::GET, &first, None).await;
    assert_eq!(status, 200, "{page}");
    let ids: Vec<&Value> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| &item["event_id"])
        .collect();
    assert_eq!(ids, [&json!("1700000000003-0"), &json!("1700000000002-0")]);
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a full page names its successor");

    let next = format!("{first}&starting_after={cursor}");
    let (status, page) = leased.call(Method::GET, &next, None).await;
    assert_eq!(status, 200, "{page}");
    let ids: Vec<&Value> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| &item["event_id"])
        .collect();
    assert_eq!(ids, [&json!("1700000000001-0")]);
    assert_eq!(page["next_cursor"], Value::Null);
    leased.finish(supervisor).await;
}

/// Writes one history row for the scenario's fleet, `at` milliseconds into
/// the epoch so the order is the test's own.
async fn record_event(run: &Scenario, event: &str, actor: &str, at: i64) {
    let mut connection = run.booted.database.acquire().await.expect("a connection");
    sqlx::query(afd_events::sql::INSERT_FLEET_EVENT)
        .bind(&run.fleet)
        .bind(event)
        .bind(&run.workspace)
        .bind(actor)
        .bind("cron")
        .bind("{}")
        .bind(Option::<&str>::None)
        .bind(at)
        .bind("processed")
        .fetch_one(&mut *connection)
        .await
        .expect("the history row inserts");
}

/// Dimension 1.8. A delete removes the schedule from QStash, then the row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fleet_deletes_its_schedule() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let (_status, created) = leased.create(false).await;
    let member = format!("/{}", created["schedule_id"].as_str().expect("an id"));

    let (status, _) = leased
        .call(Method::DELETE, &leased.fenced(&member), None)
        .await;
    assert_eq!(status, 204);
    assert_eq!(
        qstash.told(),
        [
            Told::Upsert(WEEKLY.to_owned()),
            Told::Delete("scd_fixture_1".to_owned())
        ],
        "the key QStash issued is the key it is told to remove"
    );
    assert_eq!(leased.held(None).await, 0);
    leased.finish(supervisor).await;
}

/// Dimension 1.9. A `once` schedule run now retires: QStash removes it and the
/// row goes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_once_schedule_retires_after_fire() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let (_status, created) = leased.create(true).await;
    assert_eq!(created["once"], true);
    let schedule = created["schedule_id"].as_str().expect("an id");
    let runs = leased.path(&format!("/{schedule}/runs"));

    let (status, run) = leased
        .call(
            Method::POST,
            &runs,
            Some(&json!({"fencing_token": leased.fence})),
        )
        .await;
    assert_eq!(status, 201, "{run}");
    assert_eq!(qstash.deletes(), 1, "the once schedule left QStash");
    assert_eq!(leased.held(None).await, 0, "and its row went");
    leased.finish(supervisor).await;
}

/// The REST guide's PATCH rule: the same body twice leaves the same schedule.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fleet_schedule_patch_is_idempotent() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let (_status, created) = leased.create(false).await;
    let member = format!("/{}", created["schedule_id"].as_str().expect("an id"));
    let patch = json!({"fencing_token": leased.fence, "message": "daily check", "paused": true});

    let (first_status, mut first) = leased
        .call(Method::PATCH, &leased.path(&member), Some(&patch))
        .await;
    let (second_status, mut second) = leased
        .call(Method::PATCH, &leased.path(&member), Some(&patch))
        .await;
    assert_eq!(
        (first_status, second_status),
        (200, 200),
        "{first} {second}"
    );
    assert_eq!(first["message"], "daily check");
    assert_eq!(first["status"], "paused");
    // The instant of the write is the one field a second write moves.
    for view in [&mut first, &mut second] {
        view.as_object_mut().expect("a view").remove("updated_at");
    }
    assert_eq!(first, second);
    leased.finish(supervisor).await;
}

/// Every refusal the verbs make before a schedule is touched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_runner_schedule_refusals() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let unknown = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";
    let cases: Vec<(Method, String, Option<Value>, u16, &str)> = vec![
        (
            Method::GET,
            "/v1/runners/me/leases/not-a-lease/schedules?fencing_token=1".to_owned(),
            None,
            400,
            "UZ-REQ-001",
        ),
        (
            Method::GET,
            format!("/v1/runners/me/leases/{unknown}/schedules?fencing_token=1"),
            None,
            404,
            "UZ-RUN-006",
        ),
        (Method::GET, leased.path(""), None, 400, "UZ-REQ-001"),
        (
            Method::POST,
            leased.path(""),
            Some(json!({"fencing_token": leased.fence, "cron": "* * * * * *", "message": "m"})),
            400,
            "UZ-REQ-001",
        ),
        (
            Method::POST,
            leased.path(""),
            Some(
                json!({"fencing_token": leased.fence, "cron": WEEKLY, "message": "m", "fleet_id": "x"}),
            ),
            400,
            "UZ-REQ-001",
        ),
        (
            Method::PATCH,
            leased.path(&format!("/{unknown}")),
            Some(json!({"fencing_token": leased.fence})),
            404,
            "UZ-SCHED-002",
        ),
        (
            Method::PATCH,
            leased.path("/not-a-schedule"),
            Some(json!({"fencing_token": leased.fence})),
            400,
            "UZ-REQ-001",
        ),
        (
            Method::PATCH,
            leased.path(&format!("/{unknown}")),
            Some(json!({"fencing_token": leased.fence, "timezone": "Mars/Olympus"})),
            400,
            "UZ-REQ-001",
        ),
        (
            Method::POST,
            leased.path(&format!("/{unknown}/runs")),
            Some(json!({"fencing_token": leased.fence})),
            404,
            "UZ-SCHED-002",
        ),
        (
            Method::GET,
            leased.fenced(&format!("/{unknown}/runs")),
            None,
            404,
            "UZ-SCHED-002",
        ),
        (
            Method::GET,
            format!("{}&limit=0", leased.fenced(&format!("/{unknown}/runs"))),
            None,
            400,
            "UZ-REQ-001",
        ),
        (
            Method::GET,
            format!(
                "{}&starting_after=@@",
                leased.fenced(&format!("/{unknown}/runs"))
            ),
            None,
            400,
            "UZ-REQ-001",
        ),
    ];
    for (method, path, body, status, code) in cases {
        let (answered, refusal) = leased.call(method.clone(), &path, body.as_ref()).await;
        assert_eq!(
            (answered, refusal["error_code"].as_str()),
            (status, Some(code)),
            "{method} {path}: {refusal}"
        );
    }
    assert!(qstash.told().is_empty(), "no refusal reached the scheduler");
    leased.finish(supervisor).await;
}
