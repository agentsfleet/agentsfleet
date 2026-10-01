//! Workspace stream opening over live Postgres and Dragonfly.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

#[path = "support/fleet_stream_transport.rs"]
mod transport;

#[path = "support/wall_stream_fixture.rs"]
pub(crate) mod fixture;

use std::time::Duration;

use afd_auth::scope::{Scope, ScopeSet};
use afd_dragonfly::SubscriptionHub;
use afd_sse::KIND_ACCESS_REVOKED;

use self::fixture::{Fixture, SUBJECT, Wall, data_of, next_chunk, open_stream, stream_ends};
use self::harness::Fleet;

/// More frames than the hub's per-subscriber queue (256) holds, so a body
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

/// Under the read's two-second deadline, so the paused clock reaches the
/// acquire budget first; long enough that the fixture's own real connects —
/// the seed and the opening, over the lane's TLS — are not refused by it.
const SHORT_ACQUIRE_MS: &str = "1500";

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_workspace_stream_announces_its_live_fleet_set() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let hub = SubscriptionHub::start(harness::dragonfly_config())
        .await
        .expect("the lane's subscription connection starts");
    let fleet = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_live_hub(hub.clone());
    let ownership = fleet.ownership();
    let fleet_store = fleet.fleet_store();
    let router = fleet.router();
    let mut body = open_stream(&router, &fixture).await;

    let second = fixture.seed_second_fleet().await;
    fleet_store.invalidate_live_set(&fixture.workspace).await;
    let refreshed = fleet_store
        .live_set(&fixture.workspace)
        .await
        .expect("the invalidated set refreshes before the clock is paused");
    assert!(refreshed.contains(&second));
    // Skip the tick's ten seconds on the paused clock, then let real time
    // run again before reading: the changed `hello` reads the fleets'
    // counters from Postgres, and a paused runtime that goes idle on a socket
    // auto-advances its clock — which would fire the read's own deadline
    // before the datastore answers.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    tokio::time::resume();
    let changed = next_chunk(&mut body).await;
    assert!(changed.contains("event: hello"));
    assert!(changed.contains(&second));

    ownership.revoke();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    let last = next_chunk(&mut body).await;
    assert!(
        last.contains(&format!("event: {KIND_ACCESS_REVOKED}")),
        "a revoked wall says why it is closing: {last}"
    );
    assert!(
        stream_ends(&mut body).await,
        "revoked membership closes the wall"
    );
    drop(body);
    hub.shutdown();
    tokio::time::resume();
    fixture.cleanup().await;
}

/// A `hello` whose counters read is refused still announces the set — with
/// no figures, never with zeros.
///
/// The pool holds one connection and the test keeps it, so the tick's read
/// waits on the pool; on the paused clock the runtime auto-advances to the
/// acquire budget, which is the refusal the wall handles. The set still goes
/// out (`fleet_ids` carries the fleet added since the opening) and the map is
/// empty, so a client leaves what it has standing.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_hello_whose_counters_read_is_refused_still_announces_the_set() {
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
    let refreshed = wall
        .store
        .live_set(&wall.fixture.workspace)
        .await
        .expect("the invalidated set refreshes before the connection is held");
    assert!(refreshed.contains(&second));

    // The one connection, held for the tick: the counters read can only wait.
    let held = wall
        .fixture
        .database
        .acquire()
        .await
        .expect("the pool's one connection is free to hold");
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    let changed = next_chunk(&mut wall.body).await;
    tokio::time::resume();
    drop(held);

    assert!(changed.contains("event: hello"));
    assert!(
        changed.contains(&second),
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
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_gap_is_followed_by_a_fresh_hello_with_the_fleets_counters() {
    let mut wall = Wall::open(Fixture::create().await).await;
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
