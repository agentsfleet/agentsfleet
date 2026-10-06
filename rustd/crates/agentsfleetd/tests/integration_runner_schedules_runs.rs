//! §1 — a schedule's runs: the ones that ran, and one created now, which
//! fires only what the scheduler itself would fire.
//!
//! Booted as `integration_runner_schedules` is, against a fake scheduler that
//! records what it is told.
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
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::Row as _;

use crate::e2e::Scenario;
use crate::schedules::{Leased, Seeded};
use crate::verbs::FakeQStash;
use crate::wire::poll_for_lease;

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

    let (status, first) = leased.run_now(&schedule).await;
    assert_eq!(status, 201, "{first}");
    let (status, again) = leased.run_now(&schedule).await;
    assert_eq!(status, 201, "{again}");
    assert_eq!(
        first["event_id"], again["event_id"],
        "one event's retry is one run"
    );

    let mut connection = leased.connection().await;
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

/// A run a schedule starts reaches the runner with the schedule's message as
/// its body: the lease records the fired event, which it can only do when the
/// fire stored JSON, as every other producer does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_scheduled_run_is_leased_with_its_message() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let (_status, created) = leased.create(false).await;
    let schedule = created["schedule_id"].as_str().expect("an id").to_owned();
    let (status, fired) = leased.run_now(&schedule).await;
    assert_eq!(status, 201, "{fired}");
    let event = fired["event_id"].as_str().expect("an event id").to_owned();

    leased.settle().await;
    let _woken = poll_for_lease(&leased.http, &leased.run, &event).await;

    let mut connection = leased.connection().await;
    let message: String = sqlx::query_scalar(
        "SELECT request_json->>'message' FROM core.fleet_events \
         WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(&leased.run.fleet)
    .bind(&event)
    .fetch_one(&mut *connection)
    .await
    .expect("the leased run is recorded");
    assert_eq!(json!(message), created["message"]);
    drop(connection);
    leased.finish(supervisor).await;
}

/// A person's active schedule may be run now: running is not changing it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_run_now_of_a_person_s_active_schedule_runs() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let schedule = leased.seed(Source::Api, "person-active").await;

    let (status, run) = leased.run_now(&schedule).await;
    assert_eq!(status, 201, "{run}");
    assert_eq!(leased.admitted(&schedule).await, 1);
    leased.finish(supervisor).await;
}

/// A run-now overrides neither a person's pause nor a retirement: each is a
/// 409 naming the schedule's state, and nothing is admitted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_run_now_refuses_a_schedule_that_would_not_fire() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    for (source, state) in [(Source::Api, "paused"), (Source::Fleet, "deleting")] {
        let row = Seeded {
            desired_status: state,
            ..Seeded::active(source, state)
        };
        let schedule = leased.seed_row(&leased.run.fleet, row).await;

        let (status, refused) = leased.run_now(&schedule).await;
        assert_eq!(
            (status, &refused["error_code"], &refused["current_state"]),
            (409, &json!("UZ-SCHED-011"), &json!(state)),
            "{refused}"
        );
        assert_eq!(leased.admitted(&schedule).await, 0, "{state}");
    }
    leased.finish(supervisor).await;
}

/// A fleet an operator stopped takes no run-now, whatever its schedules say.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_run_now_refuses_a_fleet_that_takes_no_work() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let schedule = leased.seed(Source::Fleet, "own-active").await;
    leased
        .on_fleet("UPDATE core.fleets SET status = 'paused' WHERE id = $1::uuid")
        .await;

    let (status, refused) = leased.run_now(&schedule).await;
    assert_eq!(
        (status, &refused["error_code"], &refused["current_state"]),
        (409, &json!("UZ-AGT-012"), &json!("paused")),
        "{refused}"
    );
    assert_eq!(leased.admitted(&schedule).await, 0);
    leased.finish(supervisor).await;
}

/// A run a schedule started cannot run one now, so no schedule wakes its fleet
/// in a loop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_run_now_from_a_scheduled_run_is_refused() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let schedule = leased.seed(Source::Fleet, "own-active").await;
    leased.lease_woken_by_schedule().await;

    let (status, refused) = leased.run_now(&schedule).await;
    assert_eq!(
        (status, &refused["error_code"], &refused["current_state"]),
        (409, &json!("UZ-SCHED-011"), &json!("scheduled_run")),
        "{refused}"
    );
    assert_eq!(leased.admitted(&schedule).await, 0);
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
    // Another schedule's run, which this list must not carry, and a longer
    // actor this one's id is a prefix of, which an exact read never matches.
    record_event(&leased.run, "1700000000004-0", "cron:someone-else", 4).await;
    record_event(&leased.run, "1700000000005-0", &format!("{actor}0"), 5).await;

    let first = format!("{}&limit=2", leased.fenced(&format!("/{schedule}/runs")));
    let (status, page) = leased.call(Method::GET, &first, None).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(
        event_ids(&page),
        [&json!("1700000000003-0"), &json!("1700000000002-0")]
    );
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a full page names its successor");

    let next = format!("{first}&starting_after={cursor}");
    let (status, page) = leased.call(Method::GET, &next, None).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(event_ids(&page), [&json!("1700000000001-0")]);
    assert_eq!(page["next_cursor"], Value::Null);
    leased.finish(supervisor).await;
}

/// Dimension 1.9. A `once` schedule run now retires: `QStash` removes it and the
/// row goes, so a repeat finds nothing to run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_once_schedule_retires_after_fire() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let (_status, created) = leased.create(true).await;
    assert_eq!(created["once"], true);
    let schedule = created["schedule_id"].as_str().expect("an id");

    let (status, run) = leased.run_now(schedule).await;
    assert_eq!(status, 201, "{run}");
    assert_eq!(qstash.deletes(), 1, "the once schedule left QStash");
    assert_eq!(leased.held(None).await, 0, "and its row went");
    let (status, again) = leased.run_now(schedule).await;
    assert_eq!(
        (status, &again["error_code"]),
        (404, &json!("UZ-SCHED-002")),
        "{again}"
    );
    leased.finish(supervisor).await;
}

/// A `once` schedule fired while another syncer held its row: the fire is
/// admitted, the call answers 409 so it is repeated, and the repeat replays
/// the same event and finishes the retirement.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_once_retirement_held_is_repeated_not_dropped() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let row = Seeded {
        once: true,
        ..Seeded::active(Source::Fleet, "once-held")
    };
    let schedule = leased.seed_row(&leased.run.fleet, row).await;
    leased.hold_sync(&schedule, true).await;

    let (status, refused) = leased.run_now(&schedule).await;
    assert_eq!(
        (status, &refused["error_code"]),
        (409, &json!("UZ-SCHED-006")),
        "{refused}"
    );
    assert_eq!(leased.admitted(&schedule).await, 1, "the fire is durable");
    assert_eq!(leased.held(None).await, 1, "the row waits, still active");

    leased.hold_sync(&schedule, false).await;
    let (status, run) = leased.run_now(&schedule).await;
    assert_eq!(status, 201, "{run}");
    assert_eq!(
        leased.admitted(&schedule).await,
        1,
        "the repeat replayed it"
    );
    assert_eq!(leased.held(None).await, 0, "and retired the row");
    leased.finish(supervisor).await;
}

/// The event ids a runs page carries, in order.
fn event_ids(page: &Value) -> Vec<&Value> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| &item["event_id"])
        .collect()
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
