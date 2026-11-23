//! Workspace stream opening over live Postgres and the lane's NATS.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

#[path = "support/fleet_stream_transport.rs"]
mod transport;

#[path = "support/wall_stream_fixture.rs"]
pub(crate) mod fixture;

#[path = "runner_plane/native_opening.rs"]
mod native_opening;

use afd_sse::KIND_ACCESS_REVOKED;
use axum::body::BodyDataStream;

use self::fixture::{Fixture, SUBJECT, Wall, data_of, next_chunk, stream_ends};

/// More frames than the bus's per-reader buffer (256) holds, so a body
/// nobody reads falls behind and the fan-in reports the gap.
const GAP_FRAMES: usize = 400;

/// How many chunks to read looking for the gap and the greeting behind it —
/// a few, in case the first published frames land before the overflow.
const GAP_READS: usize = 8;

/// The pool knobs a deployment sets, spelled here so the refused-read test
/// configures the pool the way an operator can.
const POOL_SIZE_KNOB: &str = "DATABASE_POOL_SIZE_API";
const MIN_POOL_SIZE_KNOB: &str = "DATABASE_MIN_POOL_SIZE_API";
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";
const ONE_CONNECTION: &str = "1";

/// Under the read's two-second deadline, so the acquire budget refuses the
/// counters read first; long enough that the fixture's own real connects — the
/// seed and the opening, over the lane's TLS — are not refused by it.
const SHORT_ACQUIRE_MS: &str = "1500";

/// A revoked wall says why it is closing, and then nothing follows.
async fn assert_revoked_wall_closes(body: &mut BodyDataStream) {
    let last = next_chunk(body).await;
    assert!(
        last.contains(&format!("event: {KIND_ACCESS_REVOKED}")),
        "a revoked wall says why it is closing: {last}"
    );
    assert!(
        stream_ends(body).await,
        "revoked membership closes the wall"
    );
}

/// A fleet created after the wall opened is announced on the notice its create
/// publishes, and a member removal's notice ends a revoked caller's wall.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_workspace_stream_announces_its_live_fleet_set() {
    let mut wall = Wall::open(Fixture::create().await).await;
    let second = wall.fixture.seed_second_fleet().await;
    wall.notify_fleets().await;
    let changed = wall.next_of("hello").await;
    assert!(
        changed.contains(&second),
        "the new fleet is announced: {changed}"
    );

    wall.ownership.revoke();
    wall.notify_members().await;
    assert_revoked_wall_closes(&mut wall.body).await;
    wall.close().await;
}

/// A `hello` whose counters read is refused still announces the set — with
/// no figures, never with zeros.
///
/// The pool holds one connection and the test keeps it while a gap asks for a
/// fresh `hello`, so its counters read waits on the pool until the acquire
/// budget refuses it. The set still goes out and the map is empty, so a client
/// leaves what it has standing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_hello_whose_counters_read_is_refused_still_announces_the_set() {
    let fixture = Fixture::with_pool(&[
        (POOL_SIZE_KNOB, ONE_CONNECTION),
        (MIN_POOL_SIZE_KNOB, ONE_CONNECTION),
        (ACQUIRE_TIMEOUT_KNOB, SHORT_ACQUIRE_MS),
    ])
    .await;
    let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    let mut wall = Wall::open_over(fixture, bus).await;
    // The one connection, held: the counters read can only wait.
    let held = wall
        .fixture
        .database
        .acquire()
        .await
        .expect("the pool's one connection is free to hold");
    wall.publish(GAP_FRAMES).await;
    wall.next_of("catching_up").await;
    let changed = wall.next_of("hello").await;
    drop(held);

    assert!(
        changed.contains(wall.fixture.fleet.as_str()),
        "the set is announced whether or not it was priced"
    );
    let hello = data_of(&changed);
    assert_eq!(
        hello.pointer("/counters"),
        Some(&serde_json::json!({})),
        "a refused read sends the set without figures, never with zeros: {hello}"
    );
    wall.close().await;
}

/// A gap the server could not carry is followed by a fresh `hello`.
///
/// More frames are published than the fan-in's queue holds while nothing
/// reads the body, so the first thing read back is the `catching_up`, and
/// the second is a `hello` carrying where every fleet stands now — the
/// dropped frames are exactly the ones that moved the counters.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_gap_is_followed_by_a_fresh_hello_with_the_fleets_counters() {
    // The in-process bus overflows a reader's buffer exactly; a broker's slow
    // consumer notice arrives on its own clock, so it cannot be asserted on.
    let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    let mut wall = Wall::open_over(Fixture::create().await, bus).await;
    wall.publish(GAP_FRAMES).await;

    let mut heard = Vec::new();
    for _ in 0..GAP_READS {
        let chunk = next_chunk(&mut wall.body).await;
        if chunk.contains("event: catching_up") {
            heard.push("catching_up");
            continue;
        }
        if chunk.contains("event: hello") {
            heard.push("hello");
            let hello = data_of(&chunk);
            assert!(
                hello
                    .pointer(&format!(
                        "/counters/{}/events_processed",
                        wall.fixture.fleet
                    ))
                    .is_some(),
                "the hello after a gap carries the fleet's counters: {hello}"
            );
            break;
        }
    }
    assert_eq!(
        heard,
        ["catching_up", "hello"],
        "a gap is announced, then the set is re-announced with its figures"
    );
    wall.close().await;
}
