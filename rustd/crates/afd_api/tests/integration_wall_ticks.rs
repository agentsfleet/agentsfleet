//! What the wall's refresh tick does when it has nothing to say, when the
//! datastore behind it does not answer, and when a lagging viewer asks for
//! fresh counters faster than a tick allows.
//!
//! The tick is ten seconds of the runtime's clock. Each test pauses the clock,
//! advances it past a tick and resumes before reading, so the reads the tick
//! makes run on real time and never race the clock's auto-advance.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use crate::integration_fleet_streams::fixture::{Fixture, Wall, next_chunk};

/// Past one refresh tick of the wall.
const PAST_A_TICK: Duration = Duration::from_secs(11);

/// Frames enough to overflow a viewer's 256-frame buffer while it is not read.
const OVERFLOW_FRAMES: usize = 400;

/// How long a stream is watched for anything more before a tick is forced.
const QUIET: Duration = Duration::from_millis(1_500);

/// The pool knobs that let a test hold the only connection.
const POOL_SIZE_KNOB: &str = "DATABASE_POOL_SIZE_API";
const MIN_POOL_SIZE_KNOB: &str = "DATABASE_MIN_POOL_SIZE_API";
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";
const ONE_CONNECTION: &str = "1";
const SHORT_ACQUIRE_MS: &str = "500";

/// Moves the runtime's clock past a tick, then hands it back to real time.
async fn past_a_tick() {
    tokio::time::pause();
    tokio::time::advance(PAST_A_TICK).await;
    tokio::time::resume();
}

/// A tick that finds the set unchanged and the caller still a member says
/// nothing, and the stream goes on carrying activity.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_steady_tick_announces_nothing_and_the_stream_keeps_serving() {
    let mut wall = Wall::open(Fixture::create().await).await;
    past_a_tick().await;
    wall.stays_quiet().await;
    wall.publish(1).await;
    assert!(next_chunk(&mut wall.body).await.contains("event: chunk"));
    wall.close().await;
}

/// A tick whose ownership read fails keeps the stream open: a datastore blip
/// is not a revocation.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_tick_whose_ownership_read_fails_keeps_the_stream_open() {
    let mut wall = Wall::open(Fixture::create().await).await;
    wall.ownership.refuse();
    past_a_tick().await;
    wall.stays_quiet().await;
    wall.publish(1).await;
    assert!(next_chunk(&mut wall.body).await.contains("event: chunk"));
    wall.close().await;
}

/// A tick whose set read fails keeps the set it has — a fleet added meanwhile
/// is not announced — and the next tick that can read announces it.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_tick_whose_set_read_fails_keeps_the_set_until_one_succeeds() {
    let fixture = Fixture::with_pool(&[
        (POOL_SIZE_KNOB, ONE_CONNECTION),
        (MIN_POOL_SIZE_KNOB, ONE_CONNECTION),
        (ACQUIRE_TIMEOUT_KNOB, SHORT_ACQUIRE_MS),
    ])
    .await;
    let mut wall = Wall::open(fixture).await;
    let second = wall.fixture.seed_second_fleet().await;
    wall.store
        .invalidate_live_set(&wall.fixture.workspace)
        .await;

    let held = wall
        .fixture
        .database
        .acquire()
        .await
        .expect("the pool's one connection is free to hold");
    past_a_tick().await;
    wall.stays_quiet().await;
    drop(held);

    past_a_tick().await;
    let hello = wall.next_of("hello").await;
    assert!(
        hello.contains(&second),
        "the next tick announces {second}: {hello}"
    );
    wall.close().await;
}

/// A second lag inside one tick does not re-read the counters at once: its
/// read waits for the tick after the first, which announces them then.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_second_lag_inside_a_tick_re_announces_when_the_tick_ends() {
    let mut wall = Wall::open(Fixture::create().await).await;
    wall.publish(OVERFLOW_FRAMES).await;
    wall.next_of("catching_up").await;
    wall.next_of("hello").await;

    wall.publish(OVERFLOW_FRAMES).await;
    wall.next_of("catching_up").await;
    while let Ok(event) = tokio::time::timeout(QUIET, next_chunk(&mut wall.body)).await {
        assert!(
            !event.contains("event: hello"),
            "no read before the tick: {event}"
        );
    }
    past_a_tick().await;
    let hello = wall.next_of("hello").await;
    assert!(
        hello.contains(&wall.fixture.fleet),
        "the owed read announces the set"
    );
    wall.close().await;
}
