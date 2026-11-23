//! The visit a connection's end schedules records a silent runner once, and
//! leaves alone a runner that beat after its connection ended.

use af_core::{clock, id::Uuid7};
use afd_api_wire::{
    admin::{AdminState, RunnerAdminAction, RunnerEventType},
    operator::CordonReason,
};
use serde_json::Value;

use super::{Fixture, beat, connected, silence};

/// Who the operator drain is recorded as.
const ACTOR: &str = "user_operator";

async fn count(fixture: &Fixture, runner: &Uuid7, event_type: RunnerEventType) -> usize {
    fixture
        .history(runner)
        .await
        .iter()
        .filter(|event| event.event_type == event_type)
        .count()
}

async fn status(fixture: &Fixture, runner: &Uuid7) -> (AdminState, Option<CordonReason>) {
    let detail = fixture
        .runners
        .platform()
        .runner_detail(runner)
        .await
        .expect("detail");
    let item = detail.item();
    (item.status(), item.cordon().map(|cordon| cordon.reason))
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_runner_silent_past_the_offline_window_is_cordoned_once() {
    let mut fixture = Fixture::create().await;
    let owner = connected(&mut fixture).await;
    let runner = owner.runner();
    beat(&fixture, &owner).await;
    silence(&fixture, runner).await;
    let seen = fixture
        .runners
        .last_seen(runner)
        .await
        .expect("the beat reads")
        .expect("a runner that beat has a last beat");

    fixture
        .runners
        .record_silence(runner, seen, clock::now())
        .await
        .expect("the visit");
    assert_eq!(
        status(&fixture, runner).await,
        (AdminState::Active, Some(CordonReason::MissedHeartbeats)),
        "quarantined, with the operator status left alone"
    );
    let history = fixture.history(runner).await;
    let offline = history
        .iter()
        .find(|event| event.event_type == RunnerEventType::RunnerOffline)
        .expect("the offline row");
    assert_eq!(
        offline.metadata.get("last_seen_at"),
        Some(&Value::from(seen.to_string())),
        "the row names the beat the episode began after"
    );
    let cordoned = history
        .iter()
        .find(|event| event.event_type == RunnerEventType::RunnerCordoned)
        .expect("the cordon row");
    assert_eq!(
        cordoned.metadata.get("reason"),
        Some(&Value::from(CordonReason::MissedHeartbeats.as_str()))
    );

    fixture
        .runners
        .record_silence(runner, seen, clock::now())
        .await
        .expect("a second visit to the same episode");
    assert_eq!(
        count(&fixture, runner, RunnerEventType::RunnerOffline).await,
        1
    );
    assert_eq!(
        count(&fixture, runner, RunnerEventType::RunnerCordoned).await,
        1
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_runner_that_beat_again_is_left_alone() {
    let mut fixture = Fixture::create().await;
    let owner = connected(&mut fixture).await;
    let runner = owner.runner();
    beat(&fixture, &owner).await;
    silence(&fixture, runner).await;
    let seen = fixture
        .runners
        .last_seen(runner)
        .await
        .expect("the beat reads")
        .expect("a runner that beat has a last beat");
    beat(&fixture, &owner).await;

    fixture
        .runners
        .record_silence(runner, seen, clock::now())
        .await
        .expect("the visit");
    assert_eq!(
        status(&fixture, runner).await,
        (AdminState::Active, None),
        "no cordon on a runner that came back"
    );
    assert_eq!(
        count(&fixture, runner, RunnerEventType::RunnerOffline).await,
        0
    );
    assert_eq!(
        count(&fixture, runner, RunnerEventType::RunnerCordoned).await,
        0
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_runner_that_never_beat_has_no_offline_instant() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;

    assert_eq!(
        fixture.runners.last_seen(&runner).await.expect("the read"),
        None,
        "a runner that never beat has not stopped"
    );
    fixture
        .runners
        .record_silence(&runner, afd_runner::sql::NEVER_SEEN, clock::now())
        .await
        .expect("the visit");
    assert_eq!(status(&fixture, &runner).await, (AdminState::Active, None));
    assert_eq!(
        count(&fixture, &runner, RunnerEventType::RunnerOffline).await,
        0,
        "the never-seen sentinel names no episode"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_silent_draining_runner_finishes_its_drain_without_a_cordon() {
    let mut fixture = Fixture::create().await;
    let owner = connected(&mut fixture).await;
    let runner = owner.runner();
    beat(&fixture, &owner).await;
    fixture
        .runners
        .platform()
        .transition(runner, RunnerAdminAction::Drain, ACTOR, clock::now())
        .await
        .expect("a drain applies");
    silence(&fixture, runner).await;
    let seen = fixture
        .runners
        .last_seen(runner)
        .await
        .expect("the beat reads")
        .expect("a runner that beat has a last beat");

    fixture
        .runners
        .record_silence(runner, seen, clock::now())
        .await
        .expect("the visit");
    assert_eq!(status(&fixture, runner).await, (AdminState::Drained, None));
    assert_eq!(
        count(&fixture, runner, RunnerEventType::RunnerOffline).await,
        1
    );
    assert_eq!(
        count(&fixture, runner, RunnerEventType::RunnerCordoned).await,
        0,
        "only an active runner is cordoned"
    );
    let history = fixture.history(runner).await;
    let drained = history
        .iter()
        .find(|event| event.event_type == RunnerEventType::RunnerDrained)
        .expect("the drained row");
    assert_eq!(
        drained.metadata.get("from_status"),
        Some(&Value::from("draining"))
    );
    assert_eq!(
        drained.metadata.get("to_status"),
        Some(&Value::from("drained"))
    );
    fixture.cleanup().await;
}
