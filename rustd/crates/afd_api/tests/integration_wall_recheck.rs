//! The wall's membership re-check against a slow or absent datastore: an
//! overrun delays a revocation by one tick, and a run of unanswered ticks
//! closes the wall without `access_revoked`.
//!
//! The re-check is the one the fleet stream runs, whose unit tests pin each
//! arm; these prove the wall is wired through it. Each test pauses the
//! clock, advances it past a tick and resumes before reading, so the reads a
//! tick makes run on real time.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_dragonfly::SubscriptionHub;
use afd_sse::{HEARTBEAT_EVENT, KIND_ACCESS_REVOKED};
use axum::body::BodyDataStream;
use futures_util::StreamExt as _;
use http::{Method, StatusCode};

use crate::harness::{self, Fleet, send};
use crate::integration_fleet_streams::fixture::{Fixture, Wall, next_chunk, stream_ends};
use crate::integration_workspace_members::fixture::{Members, owner_scopes};

/// Past one refresh tick of the wall.
const PAST_A_TICK: Duration = Duration::from_secs(11);

/// How long a stream is read to show it says nothing; under `next_chunk`'s
/// own two-second patience.
const QUIET: Duration = Duration::from_millis(1_500);

/// How long a wall that should close is given to close.
const CLOSING: Duration = Duration::from_secs(5);

/// The unanswered ticks in a row that close a wall: the daemon's ceiling,
/// pinned here because a client observes it.
const UNANSWERED_TICKS: usize = 4;

/// The events the re-check logs, as an operator searches for them.
const EVENT_DEFERRED: &str = "stream_recheck_deferred";
const EVENT_UNVERIFIED: &str = "stream_closed_unverified";

/// The reason a re-check that overran its budget is logged with.
const REASON_BUDGET: &str = "recheck exceeded its budget";

/// The pool knobs that let a test hold the only connection. The acquire
/// timeout sits well past the re-check's half-second budget, so the budget
/// is what answers.
const POOL_SIZE_KNOB: &str = "DATABASE_POOL_SIZE_API";
const MIN_POOL_SIZE_KNOB: &str = "DATABASE_MIN_POOL_SIZE_API";
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";
const ONE_CONNECTION: &str = "1";
const SLOW_ACQUIRE_MS: &str = "3000";

/// Moves the runtime's clock past a tick, then hands it back to real time.
async fn past_a_tick() {
    tokio::time::pause();
    tokio::time::advance(PAST_A_TICK).await;
    tokio::time::resume();
}

/// Everything `body` sends until it ends, which it must do promptly.
async fn rest_of(body: &mut BodyDataStream) -> String {
    let mut rest = String::new();
    while let Some(chunk) = tokio::time::timeout(CLOSING, body.next())
        .await
        .expect("the wall closes")
    {
        let chunk = chunk.expect("the SSE body is infallible");
        rest.push_str(std::str::from_utf8(&chunk).expect("SSE is UTF-8"));
    }
    rest
}

/// The next event that is not a heartbeat, or the third heartbeat running:
/// a tick past the heartbeat's due time can let one out first.
async fn past_heartbeats(body: &mut BodyDataStream) -> String {
    let heartbeat = format!("event: {HEARTBEAT_EVENT}");
    let mut event = next_chunk(body).await;
    for _beat in 0..2 {
        if !event.contains(&heartbeat) {
            break;
        }
        event = next_chunk(body).await;
    }
    event
}

/// A datastore that stays down closes the wall after the ceiling's ticks,
/// with no `access_revoked`: an outage is not a revocation, and the client's
/// reconnect is authorized at open. The closing is logged once.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_wall_whose_rechecks_go_unanswered_closes_without_access_revoked() {
    let capture = Capture::install();
    let mut wall = Wall::open(Fixture::create().await).await;
    wall.ownership.refuse();
    for _tick in 1..UNANSWERED_TICKS {
        past_a_tick().await;
        wall.stays_quiet().await;
    }
    let deferred = capture
        .events()
        .into_iter()
        .filter(|record| record.field("event") == Some(EVENT_DEFERRED));
    assert_eq!(
        deferred.count(),
        UNANSWERED_TICKS - 1,
        "each tick but the last defers"
    );

    past_a_tick().await;
    let rest = rest_of(&mut wall.body).await;
    assert!(
        !rest.contains(&format!("event: {KIND_ACCESS_REVOKED}")),
        "an outage is not a revocation: {rest}"
    );
    let closing = capture.only(EVENT_UNVERIFIED);
    assert_eq!(closing.level, tracing::Level::WARN);
    assert_eq!(
        closing.field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert_eq!(
        closing.field("workspace_id"),
        Some(wall.fixture.workspace.as_str())
    );
    drop(capture);
    wall.close().await;
}

/// A pool of one connection, whose acquire waits well past the re-check's
/// budget: a test holding the connection stalls every read behind it.
async fn pool_of_one(lane: &TestDatabase) -> Db {
    lane.open(
        DbRole::Api,
        &[
            (POOL_SIZE_KNOB, ONE_CONNECTION),
            (MIN_POOL_SIZE_KNOB, ONE_CONNECTION),
            (ACQUIRE_TIMEOUT_KNOB, SLOW_ACQUIRE_MS),
        ],
    )
    .await
}

/// Bob's wall on John's workspace, past its opening `hello`, with access
/// decided by the production resolver over `pool`.
async fn bobs_wall(members: &Members, pool: &Db, hub: &SubscriptionHub) -> BodyDataStream {
    let bob = &members.bob;
    let router = Fleet::live(pool.clone(), &bob.subject, owner_scopes())
        .with_live_ownership()
        .with_dashboard_holding(&bob.subject, owner_scopes())
        .with_live_hub(hub.clone())
        .router();
    let path = format!(
        "/v1/workspaces/{}/events/stream",
        members.john.workspace.as_str()
    );
    let response = send(&router, Method::GET, &path, Some(&bob.token), "").await;
    assert_eq!(response.status(), StatusCode::OK, "Bob opens John's wall");
    let mut body = response.into_body().into_data_stream();
    assert!(next_chunk(&mut body).await.contains("event: hello"));
    body
}

/// A tick whose re-check waits on a pool past its budget keeps the wall and
/// says why; the member's removal then lands on the next tick.
///
/// The decision is the production resolver over a pool of one connection,
/// which the test holds through the first tick.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_revocation_behind_an_overrun_tick_lands_on_the_next_tick() {
    let capture = Capture::install();
    let members = Members::create().await;
    let lane = TestDatabase::shared();
    let pool = pool_of_one(&lane).await;
    let hub = harness::live_hub().await;
    let mut body = bobs_wall(&members, &pool, &hub).await;

    let held = pool
        .acquire()
        .await
        .expect("the pool's one connection is free to hold");
    past_a_tick().await;
    let overrun = tokio::time::timeout(QUIET, next_chunk(&mut body)).await;
    assert!(
        overrun.is_err(),
        "the overrun tick sends nothing: {overrun:?}"
    );
    drop(held);
    let deferred = capture.only(EVENT_DEFERRED);
    assert_eq!(deferred.field("reason"), Some(REASON_BUDGET));

    members.remove_bob().await;
    past_a_tick().await;
    let last = past_heartbeats(&mut body).await;
    assert!(
        last.contains(&format!("event: {KIND_ACCESS_REVOKED}")),
        "{last}"
    );
    assert!(
        stream_ends(&mut body).await,
        "nothing follows access_revoked"
    );
    drop(capture);
    hub.shutdown();
    drop(pool);
    lane.cleanup().await;
    members.cleanup().await;
}
