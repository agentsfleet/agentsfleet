//! §1 — a running fleet changes and deletes only its own schedules, and only
//! its own fleet's.
//!
//! Booted as `integration_runner_schedules` is, against a fake scheduler that
//! records what it is told.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just built"
)]

use afd_cron::Source;
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};

use crate::schedules::{Assigned, Seeded, WEEKLY};
use crate::verbs::{FakeQStash, Told};

/// Dimension 1.4. A person's schedule is the fleet's to read, not to change or
/// delete.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_fleet_cannot_touch_human_schedules() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Assigned::boot(&mut supervisor, &qstash).await;
    for source in [Source::Api, Source::Trigger] {
        let schedule = held
            .seed(source, &format!("person-{}", source.as_str()))
            .await;
        let member = format!("/{schedule}");
        let patch = json!({"fencing_token": held.fence, "message": "taken over"});
        let (status, refused) = held
            .call(Method::PATCH, &held.path(&member), Some(&patch))
            .await;
        assert_eq!(
            (status, &refused["error_code"]),
            (403, &json!("UZ-SCHED-010")),
            "{refused}"
        );
        let (status, refused) = held.call(Method::DELETE, &held.fenced(&member), None).await;
        assert_eq!(
            (status, &refused["error_code"]),
            (403, &json!("UZ-SCHED-010")),
            "{refused}"
        );
    }
    let (_status, listed) = held.call(Method::GET, &held.fenced(""), None).await;
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
    assert!(qstash.told().is_empty(), "{:?}", qstash.told());
    held.finish(supervisor).await;
}

/// Invariant: a run reaches only its own fleet's schedules. Another fleet's
/// schedule answers every verb exactly as one that never existed, and nothing
/// is admitted, changed or told to the scheduler.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_fleet_cannot_reach_another_fleets_schedule() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Assigned::boot(&mut supervisor, &qstash).await;
    let other = held.other_fleet().await;
    let foreign = held
        .seed_row(&other, Seeded::active(Source::Fleet, "foreign"))
        .await;
    let member = format!("/{foreign}");
    let patch = json!({"fencing_token": held.fence, "message": "taken over"});

    let cases = [
        (
            Method::POST,
            held.path(&format!("{member}/runs")),
            Some(held.fence_body()),
        ),
        (Method::PATCH, held.path(&member), Some(patch)),
        (Method::DELETE, held.fenced(&member), None),
        (Method::GET, held.fenced(&format!("{member}/runs")), None),
    ];
    for (method, path, body) in cases {
        let (status, refused) = held.call(method.clone(), &path, body.as_ref()).await;
        assert_eq!(
            (status, &refused["error_code"]),
            (404, &json!("UZ-SCHED-002")),
            "{method} {path}: {refused}"
        );
    }
    assert_eq!(held.admitted(&foreign).await, 0, "no fire was admitted");
    assert!(qstash.told().is_empty(), "{:?}", qstash.told());
    held.finish_with(supervisor, &other).await;
}

/// Dimension 1.8. A delete removes the schedule from `QStash`, then the row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_fleet_deletes_its_schedule() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Assigned::boot(&mut supervisor, &qstash).await;
    let (_status, created) = held.create(false).await;
    let schedule_id = created["schedule_id"].as_str().expect("an id");
    let member = format!("/{schedule_id}");

    let (status, _) = held.call(Method::DELETE, &held.fenced(&member), None).await;
    assert_eq!(status, 204);
    assert_eq!(
        qstash.told(),
        [
            Told::Upsert(WEEKLY.to_owned()),
            Told::Delete(format!("fleet-schedule-{schedule_id}"))
        ],
        "the id the schedule was registered under is the id it is removed by"
    );
    assert_eq!(held.held(None).await, 0);
    held.finish(supervisor).await;
}

/// The REST guide's PATCH rule: the same body twice leaves the same schedule.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_fleet_schedule_patch_is_idempotent() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Assigned::boot(&mut supervisor, &qstash).await;
    let (_status, created) = held.create(false).await;
    let member = format!("/{}", created["schedule_id"].as_str().expect("an id"));
    let patch = json!({"fencing_token": held.fence, "message": "daily check", "paused": true});

    let (first_status, mut first) = held
        .call(Method::PATCH, &held.path(&member), Some(&patch))
        .await;
    let (second_status, mut second) = held
        .call(Method::PATCH, &held.path(&member), Some(&patch))
        .await;
    assert_eq!(
        (first_status, second_status),
        (200, 200),
        "{first} {second}"
    );
    assert_eq!(first["message"], "daily check");
    assert_eq!(first["status"], "paused");
    // A second write moves two fields: its instant, and the version it mints,
    // which a later If-Match must name. Everything else is unchanged.
    assert_ne!(first["etag"], second["etag"], "each write is a new version");
    for view in [&mut first, &mut second] {
        let view = view.as_object_mut().expect("a view");
        view.remove("updated_at");
        view.remove("etag");
    }
    assert_eq!(first, second);

    // A patch that names no message keeps the stored one.
    let resume = json!({"fencing_token": held.fence, "paused": false});
    let (resumed_status, resumed) = held
        .call(Method::PATCH, &held.path(&member), Some(&resume))
        .await;
    assert_eq!(resumed_status, 200, "{resumed}");
    assert_eq!(resumed["status"], "active");
    assert_eq!(resumed["message"], "daily check");
    held.finish(supervisor).await;
}
