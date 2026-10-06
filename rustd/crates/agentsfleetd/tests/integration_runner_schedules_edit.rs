//! §1 — a running fleet changes and deletes only its own schedules, and only
//! its own fleet's.
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

use afd_cron::Source;
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};

use crate::schedules::{Leased, Seeded, WEEKLY};
use crate::verbs::{FakeQStash, Told};

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

/// Invariant: a lease reaches only its own fleet's schedules. Another fleet's
/// schedule answers every verb exactly as one that never existed, and nothing
/// is admitted, changed or told to the scheduler.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fleet_cannot_reach_another_fleets_schedule() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    let other = leased.other_fleet().await;
    let foreign = leased
        .seed_row(&other, Seeded::active(Source::Fleet, "foreign"))
        .await;
    let member = format!("/{foreign}");
    let patch = json!({"fencing_token": leased.fence, "message": "taken over"});

    let cases = [
        (
            Method::POST,
            leased.path(&format!("{member}/runs")),
            Some(leased.fence_body()),
        ),
        (Method::PATCH, leased.path(&member), Some(patch)),
        (Method::DELETE, leased.fenced(&member), None),
        (Method::GET, leased.fenced(&format!("{member}/runs")), None),
    ];
    for (method, path, body) in cases {
        let (status, refused) = leased.call(method.clone(), &path, body.as_ref()).await;
        assert_eq!(
            (status, &refused["error_code"]),
            (404, &json!("UZ-SCHED-002")),
            "{method} {path}: {refused}"
        );
    }
    assert_eq!(leased.admitted(&foreign).await, 0, "no fire was admitted");
    assert!(qstash.told().is_empty());
    leased.finish_with(supervisor, &other).await;
}

/// Dimension 1.8. A delete removes the schedule from `QStash`, then the row.
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

    // A patch that names no message keeps the stored one.
    let resume = json!({"fencing_token": leased.fence, "paused": false});
    let (resumed_status, resumed) = leased
        .call(Method::PATCH, &leased.path(&member), Some(&resume))
        .await;
    assert_eq!(resumed_status, 200, "{resumed}");
    assert_eq!(resumed["status"], "active");
    assert_eq!(resumed["message"], "daily check");
    leased.finish(supervisor).await;
}
