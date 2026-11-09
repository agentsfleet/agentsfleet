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

use af_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use af_core::id::Uuid7;
use afd_bus::{Handle, Received, Subject, Subscription};
use serde_json::Value;

use crate::e2e::Scenario;
use crate::wire::{capable_beat, wait_for_seeded_offer};

/// How long a published frame is given to reach the subscriber.
///
/// Generous, and it costs nothing when the frame arrives: the wait is a
/// `timeout` around a `recv`, so a healthy publish returns as soon as the pump
/// forwards it and only a genuine drop pays the whole budget. A tight bound
/// here would make the suite fail on a loaded machine and read as a lost frame.
pub(crate) const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// How long a subscription is given to reach every node of the lane's NATS
/// cluster before the suite publishes; see [`settle`].
pub(crate) const SUBSCRIBE_SETTLE: Duration = Duration::from_millis(250);

/// How long the "nothing was published" arms wait before believing it.
///
/// Shorter than [`FRAME_DEADLINE`] on purpose, and the asymmetry is deliberate:
/// waiting for an ABSENCE costs the full budget every single time, so a
/// negative arm that used the same five seconds would triple the suite's
/// runtime to prove something that is already decided by the time the response
/// status is read.
pub(crate) const SILENCE_WINDOW: Duration = Duration::from_millis(750);

/// One fleet's live tail on the lane's bus, subscribed as the dashboard's
/// stream subscribes, with the connection held for as long as the tail is read.
pub(crate) struct Tail {
    _bus: Handle,
    subscription: Subscription,
}

/// Subscribe to `fleet`'s tail. No namespace: the daemon a scenario boots
/// reads only the environment the scenario hands it, which sets none.
pub(crate) async fn tail_of(fleet: &Uuid7) -> Tail {
    let seeds = ProcessEnv
        .get("TEST_NATS_URL")
        .expect("TEST_NATS_URL is unset: run through make test-integration-rustd");
    let env = MapEnv::from_pairs([(afd_bus::nats::URL_ENV, seeds.as_str())]);
    let config = afd_bus::nats::Config::from_env(&env).expect("the lane publishes usable seeds");
    let bus = afd_bus::nats::connect(&config)
        .await
        .expect("the lane's NATS accepts a subscriber");
    let subscription = bus
        .subscribe(&Subject::FleetTail(fleet.clone()))
        .await
        .expect("the lane's NATS takes the subscription");
    Tail {
        _bus: bus,
        subscription,
    }
}

/// Beats, polls, and answers the lease the seeded event produced.
///
/// Both tests need the same three requests before they can forward anything,
/// and the beat is not optional — a runner that has not proven its capabilities
/// reads degraded and the poll correctly answers no-work.
pub(crate) async fn lease(run: &Scenario) -> (String, u64) {
    let beat = crate::wire::heartbeat(run, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );

    wait_for_seeded_offer(run).await
}

/// Gives the subscription time to reach every node of the lane's NATS cluster.
///
/// `subscribe` returns once its `SUB` leaves the socket, not once the server
/// answers, and a daemon connected to another node learns of the interest
/// later still. A publish issued in the same instant can therefore be
/// legitimately missed. This is the one place in these suites where a
/// sleep is the honest tool: there is no acknowledgement to await, and the
/// alternative — publishing until something arrives — would make the
/// "nothing was published" arms unprovable.
pub(crate) async fn settle() {
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;
}

/// The next frame on the tail, as JSON, or `None` if none arrives in time.
pub(crate) async fn next_frame(tail: &mut Tail) -> Option<Value> {
    received(tail, FRAME_DEADLINE).await
}

/// Asserts nothing arrives within the shorter window.
pub(crate) async fn silence(tail: &mut Tail) -> Option<Value> {
    received(tail, SILENCE_WINDOW).await
}

/// One message off the tail within `budget`, parsed.
async fn received(tail: &mut Tail, budget: Duration) -> Option<Value> {
    let received = tokio::time::timeout(budget, tail.subscription.recv())
        .await
        .ok()?;
    let Received::Message(message) = received.expect("the subscription stays live") else {
        // A lag or gap notice is not a frame this suite published; treat it as
        // nothing having arrived rather than as the message it was waiting for.
        return None;
    };
    let payload = &message.payload;
    Some(serde_json::from_str(payload).unwrap_or_else(|_malformed| {
        panic!("the tail carried a payload that is not JSON: {payload}")
    }))
}
