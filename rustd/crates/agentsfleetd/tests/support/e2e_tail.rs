//! The live tail, as a suite watches it: the subscription's settle, the two
//! waits, and the lease that opens a run for the frames to describe.
//!
//! Support rather than a suite's own helpers because two binaries watch the
//! tail — the runner's forwarded frames and the daemon's own brackets — and a
//! wait spelled twice is two deadlines that drift.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_redis::Subscription;
use afd_redis::hub::Received;
use serde_json::{Value, json};

use crate::e2e::Scenario;
use crate::wire::{capable_beat, claim, json, post};

/// How long a published frame is given to reach the subscriber.
///
/// Generous, and it costs nothing when the frame arrives: the wait is a
/// `timeout` around a `recv`, so a healthy publish returns as soon as the pump
/// forwards it and only a genuine drop pays the whole budget. A tight bound
/// here would make the suite fail on a loaded machine and read as a lost frame.
pub(crate) const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// How long the hub's pump is given to register a subscription with Redis
/// before the suite publishes; see [`settle`].
pub(crate) const SUBSCRIBE_SETTLE: Duration = Duration::from_millis(250);

/// How long the "nothing was published" arms wait before believing it.
///
/// Shorter than [`FRAME_DEADLINE`] on purpose, and the asymmetry is deliberate:
/// waiting for an ABSENCE costs the full budget every single time, so a
/// negative arm that used the same five seconds would triple the suite's
/// runtime to prove something that is already decided by the time the response
/// status is read.
pub(crate) const SILENCE_WINDOW: Duration = Duration::from_millis(750);

/// Beats, polls, and answers the lease the seeded event produced.
///
/// Both tests need the same three requests before they can forward anything,
/// and the beat is not optional — a runner that has not proven its capabilities
/// reads degraded and the poll correctly answers no-work.
pub(crate) async fn lease(http: &reqwest::Client, run: &Scenario) -> (String, u64) {
    let beat = post(http, run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(
        beat.status().as_u16(),
        200,
        "the runner proves its capabilities"
    );

    let body = json(post(http, run, "/v1/runners/me/leases", &json!({})).await).await;
    let lease = body
        .get("lease")
        .filter(|value| !value.is_null())
        .expect("the seeded fleet is leasable");
    claim(lease)
}

/// Gives the hub's pump time to register the subscription with Redis.
///
/// `subscribe` queues a command for the pump rather than round-tripping, so a
/// publish issued in the same instant can beat the `SUBSCRIBE` to the server
/// and be legitimately missed. This is the one place in these suites where a
/// sleep is the honest tool: there is no acknowledgement to await, and the
/// alternative — publishing until something arrives — would make the
/// "nothing was published" arms unprovable.
pub(crate) async fn settle() {
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;
}

/// The next frame on the tail, as JSON, or `None` if none arrives in time.
pub(crate) async fn next_frame(tail: &mut Subscription) -> Option<Value> {
    received(tail, FRAME_DEADLINE).await
}

/// Asserts nothing arrives within the shorter window.
pub(crate) async fn silence(tail: &mut Subscription) -> Option<Value> {
    received(tail, SILENCE_WINDOW).await
}

/// One message off the tail within `budget`, parsed.
async fn received(tail: &mut Subscription, budget: Duration) -> Option<Value> {
    let received = tokio::time::timeout(budget, tail.recv()).await.ok()?;
    let Received::Message(message) = received.expect("the subscription stays live") else {
        // A lag notice is not a frame this suite published; treat it as
        // nothing having arrived rather than as the message it was waiting for.
        return None;
    };
    let payload = message.payload;
    Some(serde_json::from_str(&payload).unwrap_or_else(|_malformed| {
        panic!("the tail carried a payload that is not JSON: {payload}")
    }))
}
