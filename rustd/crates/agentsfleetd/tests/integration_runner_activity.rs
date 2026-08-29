//! Dimension 4.1 — the live tail: what reaches the channel, and what does not.
//!
//! A binary of its own beside the other two runner suites (RULE FLL, split by
//! concern): this one is about a PUBLISH, and it is the only suite here that
//! needs a subscriber running while the request is in flight.
//!
//! # Why this is proven end to end rather than as a unit
//!
//! `Plane::activity` is three decisions in a row and only the middle one is
//! visible from inside the crate: the handler refuses a body it cannot read,
//! the plane refuses a lease this runner does not hold, and only then does the
//! publish happen — best-effort, on a channel nothing durable records. A unit
//! test can prove the payload shape; it cannot prove that a malformed body is
//! refused BEFORE the publish, which is the property that matters, because the
//! alternative is a runner writing arbitrary frames into a fleet's dashboard by
//! naming a lease id it does not hold.
//!
//! # The subscriber is the production one
//!
//! `SubscriptionHub` is what the dashboard's live tail runs on. Asserting
//! through it rather than through a raw client means the test proves the frame
//! reaches the consumer that actually exists, on the channel that consumer
//! subscribes to — a raw `SUBSCRIBE` would agree with a publisher that had
//! drifted from its only reader.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, and a missing lane knob is one"
)]

use std::time::Duration;

use afd_redis::hub::Received;
use afd_redis::{Subscription, SubscriptionHub};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::{Scenario, redis_config, scenario};
use crate::wire::{capable_beat, claim, field, json, post};

/// How long a published frame is given to reach the subscriber.
///
/// Generous, and it costs nothing when the frame arrives: the wait is a
/// `timeout` around a `recv`, so a healthy publish returns as soon as the pump
/// forwards it and only a genuine drop pays the whole budget. A tight bound
/// here would make the suite fail on a loaded machine and read as a lost frame.
const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// How long the "nothing was published" arms wait before believing it.
///
/// Shorter than [`FRAME_DEADLINE`] on purpose, and the asymmetry is deliberate:
/// waiting for an ABSENCE costs the full budget every single time, so a
/// negative arm that used the same five seconds would triple the suite's
/// runtime to prove something that is already decided by the time the response
/// status is read.
const SILENCE_WINDOW: Duration = Duration::from_millis(750);

/// The tool a forwarded frame names.
const TOOL_NAME: &str = "shell";

/// The text a forwarded chunk carries.
const CHUNK_TEXT: &str = "the fleet said this out loud";

/// A lease identifier no runner holds — well-formed, so the refusal comes from
/// the OWNERSHIP check rather than from a parse in front of it.
const UNHELD_LEASE: &str = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";

/// Dimension 4.1 — one frame in, one frame on the channel; malformed, nothing.
///
/// Three arms against one scenario, in the order a wiring defect would break
/// them: the frame that should publish, the body that should be refused before
/// the publish, and the lease this runner does not hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_activity_publish() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    let (lease_id, _fence) = lease(&http, &run).await;

    // Subscribed BEFORE the first forward. Pub/sub keeps nothing for a reader
    // that arrives late, so a subscription opened after the request would prove
    // a drop that never happened.
    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", run.fleet));
    settle().await;

    // ── One frame, one publish ──────────────────────────────────────────────
    let forwarded = post(
        &http,
        &run,
        &format!("/v1/runners/me/leases/{lease_id}/activity"),
        &json!({"frames": [{"fleet_response_chunk": {"text": CHUNK_TEXT}}]}),
    )
    .await;
    assert_eq!(
        forwarded.status().as_u16(),
        202,
        "the reply acknowledges RECEIPT, not publication — the publish is \
         best-effort and happens whether or not anybody is listening"
    );

    let frame = next_frame(&mut tail)
        .await
        .expect("the frame reaches the tail");
    assert_eq!(
        field(&frame, "kind"),
        &json!("chunk"),
        "the bridge renames `fleet_response_chunk` to `chunk`, which is the one \
         rename the dashboard's own switch depends on"
    );
    assert_eq!(
        field(&frame, "text"),
        &json!(CHUNK_TEXT),
        "and carries the text through unchanged"
    );
    assert_eq!(
        field(&frame, "event_id"),
        &json!(run.event_id),
        "stamped with the EVENT the lease is executing, not the lease id — the \
         dashboard groups a fleet's tail by event"
    );

    // ── A body this daemon cannot read is refused before any publish ────────
    let malformed = post(
        &http,
        &run,
        &format!("/v1/runners/me/leases/{lease_id}/activity"),
        &json!({"frames": [{"no_such_frame": {"text": "?"}}]}),
    )
    .await;
    assert_eq!(
        malformed.status().as_u16(),
        400,
        "an unknown frame kind is a refusal, not a silent drop: the variant set \
         is closed, and a runner from a newer build must learn its frame did \
         not land"
    );
    assert_eq!(
        code_of(malformed).await,
        afd_core::error_code::INVALID_REQUEST.as_str(),
        "and it carries the registry code a runner classifies on"
    );
    assert_eq!(
        silence(&mut tail).await,
        None,
        "nothing reached the channel — the body is read BEFORE the lease is \
         loaded, so a frame this daemon cannot parse never becomes a publish"
    );

    // ── A lease this runner does not hold reaches nobody's tail ─────────────
    let unheld = post(
        &http,
        &run,
        &format!("/v1/runners/me/leases/{UNHELD_LEASE}/activity"),
        &json!({"frames": [{"fleet_response_chunk": {"text": CHUNK_TEXT}}]}),
    )
    .await;
    assert_eq!(
        unheld.status().as_u16(),
        404,
        "a lease that does not resolve to THIS runner is not found — the check \
         that stops a runner writing into a fleet's live tail by guessing an id"
    );
    assert_eq!(
        silence(&mut tail).await,
        None,
        "and nothing was published on the way to refusing it"
    );

    drop(tail);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 4.1 (failure mode) — a frame the tail cannot RENDER is dropped.
///
/// One of the two drop branches in `publish_activity`, and the one that can be
/// driven over HTTP: a `tool_call_started` whose `args_redacted` is a string
/// that does not hold JSON cannot become a `RawValue`, so the frame is skipped
/// and the loop continues. The other branch — the queue refusing the publish —
/// is a socket failure and lives in `afd_fleet`'s
/// `integration_activity_publish.rs`, which holds a Redis handle that will not
/// answer. They are separate branches with the same outcome, and both are
/// covered rather than one standing in for the other.
///
/// What this pins is the property the row names: the verb's outcome does not
/// change. A telemetry failure that turned into a 500 would make a runner treat
/// a healthy run as a failed one and terminate its child.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_activity_drops_a_frame_it_cannot_render() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    let (lease_id, _fence) = lease(&http, &run).await;

    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", run.fleet));
    settle().await;

    // Well-formed at the WIRE — `args_redacted` is a string, as the contract
    // says — and unrenderable at the bridge, because that string does not hold
    // JSON. The request is therefore accepted and the frame is lost.
    let undeliverable = post(
        &http,
        &run,
        &format!("/v1/runners/me/leases/{lease_id}/activity"),
        &json!({"frames": [{"tool_call_started": {
            "name": TOOL_NAME,
            "args_redacted": "this is not JSON",
        }}]}),
    )
    .await;
    assert_eq!(
        undeliverable.status().as_u16(),
        202,
        "a frame the tail cannot carry is still an accepted forward: the run is \
         unaffected by telemetry, which is the whole reason this verb answers \
         202 rather than reporting on the publish"
    );
    assert_eq!(
        silence(&mut tail).await,
        None,
        "and nothing was published — the drop is accounted in the log, not by \
         failing the runner's request"
    );

    // The run continues: the very next frame, renderable this time, still
    // reaches the tail. A drop that had poisoned the channel or the lease would
    // show up here and nowhere else.
    let recovered = post(
        &http,
        &run,
        &format!("/v1/runners/me/leases/{lease_id}/activity"),
        &json!({"frames": [{"fleet_response_chunk": {"text": CHUNK_TEXT}}]}),
    )
    .await;
    assert_eq!(
        recovered.status().as_u16(),
        202,
        "the next frame is accepted"
    );
    let frame = next_frame(&mut tail)
        .await
        .expect("and it reaches the tail: one dropped frame ends one frame");
    assert_eq!(field(&frame, "text"), &json!(CHUNK_TEXT));

    drop(tail);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Beats, polls, and answers the lease the seeded event produced.
///
/// Both tests need the same three requests before they can forward anything,
/// and the beat is not optional — a runner that has not proven its capabilities
/// reads degraded and the poll correctly answers no-work.
async fn lease(http: &reqwest::Client, run: &Scenario) -> (String, u64) {
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
async fn settle() {
    tokio::time::sleep(Duration::from_millis(250)).await;
}

/// The next frame on the tail, as JSON, or `None` if none arrives in time.
async fn next_frame(tail: &mut Subscription) -> Option<Value> {
    received(tail, FRAME_DEADLINE).await
}

/// Asserts nothing arrives within the shorter window.
async fn silence(tail: &mut Subscription) -> Option<Value> {
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

/// The registry code a refusal carries.
async fn code_of(response: reqwest::Response) -> String {
    field(&json(response).await, "error_code")
        .as_str()
        .expect("every problem envelope carries an error code")
        .to_owned()
}
