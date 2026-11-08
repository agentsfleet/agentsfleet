//! §1 — a running fleet keeps its own schedules on the daemon's plane:
//! creating, listing, the fence and the fleet's own cap.
//!
//! Each case boots the daemon against the lane's Postgres and Dragonfly, with
//! `QSTASH_URL` pointed at a fake scheduler that records what it is told, so
//! "reconciled to `QStash`" is a call this suite can count rather than a sync
//! state it infers. The fleet is always the lease's: no body here names one.
//! The edits are in `integration_runner_schedules_edit`, the runs in
//! `integration_runner_schedules_runs`.
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

use afd_cron::{FLEET_SCHEDULES_MAX, Source};
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::Value;

use crate::schedules::{KOLKATA, Leased, WEEKLY};
use crate::verbs::{FakeQStash, Told};

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

/// One run may create two schedules at once: each is keyed by its own id, so
/// neither is refused as the other's duplicate.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_one_lease_creates_two_schedules_at_once() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;

    let ((first_status, first), (second_status, second)) =
        tokio::join!(leased.create(false), leased.create(true));
    assert_eq!(
        (first_status, second_status),
        (201, 201),
        "{first} {second}"
    );
    assert_ne!(first["schedule_id"], second["schedule_id"]);
    assert_eq!(leased.held(Some(Source::Fleet)).await, 2);
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

/// Dimension 1.3. A fleet holding its sixteen refuses the seventeenth, names
/// the state, and the scheduler is never asked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_schedule_cap_refuses_with_code() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    for slot in 0..FLEET_SCHEDULES_MAX {
        leased.seed(Source::Fleet, &format!("seeded-{slot}")).await;
    }

    let (status, refused) = leased.create(false).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error_code"], "UZ-SCHED-009");
    assert_eq!(refused["current_state"], "at_capacity");
    assert_eq!(qstash.upserts(), 0);
    let held = i64::try_from(FLEET_SCHEDULES_MAX).expect("a small cap");
    assert_eq!(leased.held(Some(Source::Fleet)).await, held);
    leased.finish(supervisor).await;
}

/// The fleet's cap counts only what the fleet made: sixteen of a person's
/// schedules beside fifteen of its own still leave it one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fleet_cap_counts_only_its_own_schedules() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;
    for slot in 0..FLEET_SCHEDULES_MAX {
        leased.seed(Source::Api, &format!("person-{slot}")).await;
    }
    for slot in 1..FLEET_SCHEDULES_MAX {
        leased.seed(Source::Fleet, &format!("own-{slot}")).await;
    }

    let (status, created) = leased.create(false).await;
    assert_eq!(
        status, 201,
        "a person's schedules spend none of it: {created}"
    );
    let held = i64::try_from(FLEET_SCHEDULES_MAX).expect("a small cap");
    assert_eq!(leased.held(Some(Source::Fleet)).await, held);
    leased.finish(supervisor).await;
}

/// A scheduler that is down: the schedule is saved, answers 201 with its sync
/// state showing, and is the fleet's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_schedule_saved_when_qstash_is_down() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::refusing(503).await;
    let leased = Leased::boot(&mut supervisor, &qstash).await;

    let (status, created) = leased.create(false).await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["sync"], "failed");
    assert_eq!(created["source"], "fleet");
    assert_eq!(leased.held(Some(Source::Fleet)).await, 1);
    let listed: Value = leased.call(Method::GET, &leased.fenced(""), None).await.1;
    assert_eq!(listed["schedules"][0]["sync"], "failed");
    leased.finish(supervisor).await;
}
