//! What the wall reads, and when: nothing while idle, the caller's membership
//! and the set on a notice, the set it has when a read fails, and a lagging
//! viewer's counters no faster than the pace allows.
//!
//! Where a test moves the runtime's clock it pauses it, advances and resumes
//! before reading, so the reads run on real time and never race the clock's
//! auto-advance.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use crate::integration_fleet_streams::fixture::{Fixture, Wall, next_chunk};

/// Past the pace between two counter reads a lag asks for.
const PAST_THE_PACE: Duration = Duration::from_secs(11);

/// An hour on the paused clock: longer than any timer the wall ran before.
const AN_HOUR: Duration = Duration::from_secs(3600);

/// Frames enough to overflow a viewer's 256-frame buffer while it is not read.
const OVERFLOW_FRAMES: usize = 400;

/// How long a stream is watched for anything more.
const QUIET: Duration = Duration::from_millis(1_500);

/// The pool knobs that let a test hold the only connection.
const POOL_SIZE_KNOB: &str = "DATABASE_POOL_SIZE_API";
const MIN_POOL_SIZE_KNOB: &str = "DATABASE_MIN_POOL_SIZE_API";
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";
const ONE_CONNECTION: &str = "1";
const SHORT_ACQUIRE_MS: &str = "500";

/// Moves the runtime's clock by `span`, then hands it back to real time.
async fn skip(span: Duration) {
    tokio::time::pause();
    tokio::time::advance(span).await;
    tokio::time::resume();
}

/// An idle wall asks nothing: an hour passes and ownership is never re-read.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_idle_wall_reads_nothing_for_an_hour() {
    let mut wall = Wall::open(Fixture::create().await).await;
    // The opening re-check after the links open, then nothing.
    wall.stays_quiet().await;
    let before = wall.ownership.authorize_calls();
    skip(AN_HOUR).await;
    wall.stays_quiet().await;
    assert_eq!(
        wall.ownership.authorize_calls(),
        before,
        "no timer re-reads the caller's membership"
    );
    wall.close().await;
}

/// A notice that finds the set unchanged and the caller still a member says
/// nothing, and the stream goes on carrying activity.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_notice_that_changes_nothing_announces_nothing() {
    let mut wall = Wall::open(Fixture::create().await).await;
    let before = wall.ownership.authorize_calls();
    wall.notify_members().await;
    wall.stays_quiet().await;
    assert!(
        wall.ownership.authorize_calls() > before,
        "the notice re-reads the caller's membership"
    );
    wall.publish(1).await;
    assert!(next_chunk(&mut wall.body).await.contains("event: chunk"));
    wall.close().await;
}

/// A notice whose set read fails keeps the set the wall has — a fleet added
/// meanwhile is not announced — and the next notice that can read announces it.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_notice_whose_set_read_fails_keeps_the_set_until_one_succeeds() {
    let fixture = Fixture::with_pool(&[
        (POOL_SIZE_KNOB, ONE_CONNECTION),
        (MIN_POOL_SIZE_KNOB, ONE_CONNECTION),
        (ACQUIRE_TIMEOUT_KNOB, SHORT_ACQUIRE_MS),
    ])
    .await;
    let mut wall = Wall::open(fixture).await;
    let second = wall.fixture.seed_second_fleet().await;

    let held = wall
        .fixture
        .database
        .acquire()
        .await
        .expect("the pool's one connection is free to hold");
    wall.notify_fleets().await;
    wall.stays_quiet().await;
    drop(held);

    wall.notify_fleets().await;
    let hello = wall.next_of("hello").await;
    assert!(
        hello.contains(&second),
        "the next notice announces {second}: {hello}"
    );
    wall.close().await;
}

/// A second lag inside the pace does not re-read the counters at once: its
/// read waits for the pace to end, which announces them then.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_second_lag_inside_the_pace_re_announces_when_it_ends() {
    let mut wall = Wall::open(Fixture::create().await).await;
    wall.publish(OVERFLOW_FRAMES).await;
    wall.next_of("catching_up").await;
    wall.next_of("hello").await;

    wall.publish(OVERFLOW_FRAMES).await;
    wall.next_of("catching_up").await;
    while let Ok(event) = tokio::time::timeout(QUIET, next_chunk(&mut wall.body)).await {
        assert!(
            !event.contains("event: hello"),
            "no read before the pace ends: {event}"
        );
    }
    skip(PAST_THE_PACE).await;
    let hello = wall.next_of("hello").await;
    assert!(
        hello.contains(wall.fixture.fleet.as_str()),
        "the owed read announces the set"
    );
    wall.close().await;
}
