//! Dimension 5.1 — what a live hub actually delivers, and in what order.
//!
//! The datastore-free half of this dimension is already pinned by `afd_sse`'s
//! own unit tests: a connection counts from zero, a control frame spends no
//! number, and a lag notice arrives in band as `catching_up`. Those are
//! decisions this crate makes about values it is handed.
//!
//! What they cannot prove is that Dragonfly hands them over IN THE ORDER THEY WERE
//! PUBLISHED, because there is no publisher in a unit test — the ordering is a
//! property of the transport and of the hub's single pumped connection, and the
//! only way to observe it is to publish through one. That is this file.
//!
//! # Why `Last-Event-ID` is proven by ABSENCE
//!
//! There is nothing to assert a header against: [`tail`] takes a subscription
//! and no resume token, so the header cannot be honoured by construction. The
//! claim worth making is the one that explains WHY the API has no such
//! parameter — a second connection receives only what was published after it
//! subscribed, because pub/sub keeps nothing to resume from. Honouring the
//! header would promise a backfill this transport cannot deliver, and the
//! client recovers the gap through the events list instead. So the test
//! publishes into a gap with nobody subscribed and proves those frames are
//! gone, rather than sending a header nothing reads.
//!
//! Marked `#[ignore]` so `make test-unit-all` still COMPILES and lints these
//! without a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::Duration;

use afd_dragonfly::SubscriptionHub;
use afd_dragonfly::streams::FleetStreams;
use afd_sse::FanIn;
use afd_sse::ceiling::Ceiling;
use afd_sse::channel;
use afd_sse::frame::Frame;
use afd_sse::tail::tail;
use afd_sse::{KIND_HELLO, Live};
use futures_util::StreamExt as _;

#[path = "support/sse_lane.rs"]
mod support;

use self::support::{DELIVERY_BUDGET, SseLane};

/// How many frames the ordering claim is made over.
///
/// Eight rather than two: an out-of-order transport that swapped an adjacent
/// pair would pass a two-frame test half the time, and a run that reorders
/// nothing across eight is saying something a coin flip cannot.
const ORDERED_FRAMES: u8 = 8;

/// The `kind` the fixture payloads name as their leading field.
const KIND: &str = "run_output";

/// One publisher payload, shaped as a producer writes one.
///
/// `kind` leads, because that is the anchor [`Frame::activity`] reads the
/// `event:` line from — a payload whose shape drifted would still arrive, just
/// under the default name, and pinning the name here is what notices.
fn payload(n: u8) -> String {
    format!(r#"{{"kind":"{KIND}","n":{n}}}"#)
}

/// Dimension 5.1 — frames arrive in publish order, numbered from zero, and a
/// reconnect starts over rather than resuming.
///
/// Three claims in one connection's lifetime, because they are three facts
/// about the SAME stream and splitting them would need three live hubs to say
/// less:
///
/// 1. eight payloads published in order arrive in that order;
/// 2. their sequence numbers are `0..8` — gapless, and starting at zero;
/// 3. a second connection to the same channel numbers from zero AGAIN, and
///    receives nothing published while nobody held a subscription.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_sse_sequencing_semantics() {
    let lane = SseLane::connect().await;
    let publisher = FleetStreams::new(lane.redis.clone());
    let fleet = lane.fleet("sequencing");
    let activity = channel::activity(&fleet);

    let hub = SubscriptionHub::start(SseLane::config())
        .await
        .expect("the hub starts");

    // A primer reader, so the server-side SUBSCRIBE is known to be live before
    // anything is published for the tail to count. `subscribe` queues the
    // command and the pump issues it, so the registration is asynchronous by
    // construction: a tail built and published to immediately would miss its
    // first frames on a loaded machine and pass on an idle one, which is the
    // definition of a flake. Every LATER subscription to this channel joins the
    // broadcast that already exists, so priming once is enough for both tails.
    let mut primer = hub.subscribe(&activity);
    prime(&publisher, &activity, &mut primer).await;
    assert_ordered_frames(&publisher, &hub, &activity).await;
    assert_reconnect_starts_over(&publisher, &hub, &activity, &mut primer).await;
}

/// The per-fleet stream ANNOUNCES itself, before anything is published.
///
/// Without this the first byte of an idle fleet's body is the keep-alive
/// heartbeat, `afd_sse::HEARTBEAT_INTERVAL` away. The browser opens the socket
/// at once, but the surface reports itself live off the first FRAME -- so a
/// quiet fleet rendered "Connecting…" for fifteen seconds with nothing whatever
/// wrong. The wall stream has always opened with `hello`; the per-fleet route
/// is what lacked one.
///
/// The `hello` spends no activity sequence number: it is the server talking
/// ABOUT the stream, so the first real frame is still `seq` zero. Asserted here
/// because a control frame that consumed a number would leave a gap in the ids
/// a client uses to tell a dropped frame from a control one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_fleet_stream_opens_with_hello_before_any_activity() {
    let lane = SseLane::connect().await;
    let publisher = FleetStreams::new(lane.redis.clone());
    let fleet = lane.fleet("hello");
    let activity = channel::activity(&fleet);

    let hub = SubscriptionHub::start(SseLane::config())
        .await
        .expect("the hub starts");
    let mut primer = hub.subscribe(&activity);
    prime(&publisher, &activity, &mut primer).await;

    let live = Live::new(hub, Ceiling::new(1));
    let mut stream = Box::pin(live.tail_of(&fleet));

    // Nothing is published before this read. The frame must arrive anyway --
    // that IS the claim, and a test that published first would pass on the
    // behaviour it exists to refuse.
    let hello = next_frame(&mut stream).await;
    assert_eq!(
        hello.kind, KIND_HELLO,
        "the per-fleet stream opens with `hello`, so a client watching a quiet \
         fleet is told the subscription attached instead of waiting out a \
         heartbeat interval in `Connecting…`"
    );
    assert!(
        hello.data.contains(fleet.as_str()),
        "the opening frame names the fleet it carries: {}",
        hello.data
    );
    assert_eq!(
        hello.seq, 0,
        "a control frame rides the synthetic sequence, never the connection's"
    );

    publisher
        .publish(&activity, &payload(0))
        .await
        .expect("the publish reaches Dragonfly");
    let first = next_frame(&mut stream).await;
    assert_eq!(
        first.seq, 0,
        "the hello spent no activity number -- the first real frame is still zero"
    );
    assert_eq!(first.data, payload(0));
}

async fn assert_ordered_frames(publisher: &FleetStreams, hub: &SubscriptionHub, activity: &str) {
    let mut first = Box::pin(tail(hub.subscribe(activity)));

    for n in 0..ORDERED_FRAMES {
        publisher
            .publish(activity, &payload(n))
            .await
            .expect("the publish reaches Dragonfly");
    }

    for n in 0..ORDERED_FRAMES {
        let frame = next_frame(&mut first).await;
        assert_eq!(
            frame.seq,
            u64::from(n),
            "the connection numbers its frames from zero, without gaps"
        );
        assert_eq!(frame.kind, KIND, "the event name is read from the payload");
        assert_eq!(
            frame.data,
            payload(n),
            "frame {n} arrived out of publish order, or rewritten"
        );
    }
}

async fn assert_reconnect_starts_over(
    publisher: &FleetStreams,
    hub: &SubscriptionHub,
    activity: &str,
    primer: &mut afd_dragonfly::Subscription,
) {
    let missed = r#"{"kind":"run_output","n":"during-the-gap"}"#;
    publisher
        .publish(activity, missed)
        .await
        .expect("the publish reaches Dragonfly");

    drain_until(primer, missed).await;

    let mut second = Box::pin(tail(hub.subscribe(activity)));
    let resumed = payload(0);
    publisher
        .publish(activity, &resumed)
        .await
        .expect("the publish reaches Dragonfly");

    let frame = next_frame(&mut second).await;
    assert_eq!(
        frame.seq, 0,
        "a reconnected stream is a NEW connection and counts from zero again"
    );
    assert_eq!(
        frame.data, resumed,
        "the reconnect must receive what was published after it subscribed, \
         and never the frame published into the gap — there is nothing to resume from"
    );
}

/// A workspace fan-in attaches only its authorised fleet set, numbers valid
/// frames across channels, drops an unrouteable payload without spending a
/// number, and detaches a fleet on the next authorization refresh.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_workspace_fan_in_tracks_authorised_fleets_and_valid_frames() {
    let lane = SseLane::connect().await;
    let publisher = FleetStreams::new(lane.redis.clone());
    let alpha = lane.fleet("fanin-alpha");
    let beta = lane.fleet("fanin-beta");
    let alpha_channel = channel::activity(&alpha);
    let beta_channel = channel::activity(&beta);
    let hub = SubscriptionHub::start(SseLane::config())
        .await
        .expect("the hub starts");

    // Prime each server-side subscription before the fan-in joins its local
    // broadcast. Once the channel exists, `subscribe` adds a receiver without
    // a second Dragonfly round trip, so no fixed sleep is involved.
    let mut alpha_primer = hub.subscribe(&alpha_channel);
    prime(&publisher, &alpha_channel, &mut alpha_primer).await;
    let mut beta_primer = hub.subscribe(&beta_channel);
    prime(&publisher, &beta_channel, &mut beta_primer).await;

    let mut fan_in = FanIn::new(Some(hub));
    let wanted = BTreeSet::from([beta.clone(), alpha.clone()]);
    let attached = fan_in.sync_to(&wanted);
    assert_eq!(attached.attached, 2);
    assert_eq!(attached.detached, 0);
    assert!(attached.is_change());
    assert_eq!(fan_in.fleets(), vec![alpha.clone(), beta.clone()]);
    assert!(!fan_in.sync_to(&wanted).is_change());
    assert_fan_in_delivery(&publisher, &mut fan_in, &alpha, &beta).await;

    let remaining = BTreeSet::from([beta.clone()]);
    let detached = fan_in.sync_to(&remaining);
    assert_eq!(detached.attached, 0);
    assert_eq!(detached.detached, 1);
    assert_eq!(fan_in.fleets(), vec![beta]);
}

async fn assert_fan_in_delivery(
    publisher: &FleetStreams,
    fan_in: &mut FanIn,
    alpha: &str,
    beta: &str,
) {
    let beta_channel = channel::activity(beta);
    publisher
        .publish(&beta_channel, &payload(3))
        .await
        .expect("a valid fan-in frame reaches Dragonfly");
    let first = tokio::time::timeout(DELIVERY_BUDGET, fan_in.next_frame())
        .await
        .expect("the fan-in yields its first frame");
    assert_eq!(first.seq, 0);
    assert!(
        first.data.contains(beta),
        "the frame is tagged by its fleet"
    );

    let alpha_channel = channel::activity(alpha);
    publisher
        .publish(&alpha_channel, "not-json")
        .await
        .expect("the malformed payload still reaches the channel");
    publisher
        .publish(&alpha_channel, &payload(4))
        .await
        .expect("the valid payload follows it");
    let second = tokio::time::timeout(DELIVERY_BUDGET, fan_in.next_frame())
        .await
        .expect("the fan-in skips the malformed payload");
    assert_eq!(second.seq, 1);
    assert!(second.data.contains(alpha));
}

/// The next frame, or a failed test rather than a hung lane.
async fn next_frame<S>(stream: &mut S) -> Frame
where
    S: futures_util::Stream<Item = Frame> + Unpin,
{
    tokio::time::timeout(DELIVERY_BUDGET, stream.next())
        .await
        .expect("a frame must arrive inside the delivery budget")
        .expect("the tail must not end while a frame is owed")
}

/// Reads `reader` forward until `payload` has gone past.
///
/// The point is the pump's progress, not the payload: once this returns, every
/// message published before `payload` has been broadcast, so a receiver created
/// afterwards is guaranteed not to see any of them.
async fn drain_until(reader: &mut afd_dragonfly::Subscription, payload: &str) {
    let deadline = tokio::time::Instant::now() + DELIVERY_BUDGET;
    loop {
        match tokio::time::timeout(DELIVERY_BUDGET, reader.recv()).await {
            Ok(Ok(afd_dragonfly::hub::Received::Message(message)))
                if message.payload == payload =>
            {
                return;
            }
            Ok(Ok(_other)) => {}
            Ok(Err(closed)) => panic!("the hub closed while draining: {closed}"),
            Err(_elapsed) => {}
        }

        assert!(
            tokio::time::Instant::now() < deadline,
            "{payload:?} never reached the primer"
        );
    }
}

/// Publishes until `reader` sees it, so the server-side subscription is known
/// live.
///
/// Republishing rather than sleeping: the wait is on a registration happening
/// on a server and in another task, with no handshake to await, and a fixed
/// sleep is either too short on a loaded runner or wasted on an idle one.
async fn prime(publisher: &FleetStreams, activity: &str, reader: &mut afd_dragonfly::Subscription) {
    let marker = r#"{"kind":"primer"}"#;
    let deadline = tokio::time::Instant::now() + DELIVERY_BUDGET;
    loop {
        publisher
            .publish(activity, marker)
            .await
            .expect("the publish reaches Dragonfly");

        match tokio::time::timeout(Duration::from_millis(100), reader.recv()).await {
            Ok(Ok(afd_dragonfly::hub::Received::Message(message))) if message.payload == marker => {
                return;
            }
            Ok(Ok(_other)) => {}
            Ok(Err(closed)) => panic!("the hub closed while priming: {closed}"),
            Err(_elapsed) => {}
        }

        assert!(
            tokio::time::Instant::now() < deadline,
            "the subscription never went live on {activity}"
        );
    }
}
