//! The wall's membership re-check against an absent datastore: an unanswered
//! tick closes the wall without `access_revoked`.
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

use af_core::error_code;
use af_core::test_util::trace::Capture;
use afd_sse::KIND_ACCESS_REVOKED;
use axum::body::BodyDataStream;
use futures_util::StreamExt as _;

use crate::integration_fleet_streams::fixture::{Fixture, Wall};

/// Past one refresh tick of the wall.
const PAST_A_TICK: Duration = Duration::from_secs(11);

/// How long a wall that should close is given to close.
const CLOSING: Duration = Duration::from_secs(5);

/// The event the re-check logs, as an operator searches for it.
const EVENT_UNVERIFIED: &str = "stream_closed_unverified";

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

/// A datastore that does not answer closes the wall on that tick, with no
/// `access_revoked`: an outage is not a revocation, and the client's
/// reconnect is authorized at open. The closing is logged once.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_wall_whose_rechecks_go_unanswered_closes_without_access_revoked() {
    let capture = Capture::install();
    let mut wall = Wall::open(Fixture::create().await).await;
    wall.ownership.refuse();
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
    assert_eq!(
        closing.field("stream"),
        Some("wall"),
        "the record names its stream"
    );
    drop(capture);
    wall.close().await;
}
