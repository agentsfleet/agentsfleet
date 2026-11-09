//! Dimension 5.1 — what a tail delivers over the bus, and in what order.
//!
//! The frame-level half of this dimension is pinned by `afd_sse`'s own unit
//! tests: a lag or a gap arrives in band as `catching_up` and spends no
//! number. Those hand a tail one report at a time.
//!
//! These publish a run of payloads through a bus [`Handle`], as a producer
//! does, and read them back off the tail and the fan-in: publish order
//! survives to the client, a connection numbers its frames from zero without
//! gaps, and a second connection starts again from zero. The bus is the
//! in-process one, so what is graded is the tail's own order and numbering;
//! that the NATS adapter keeps a subject's publish order is `afd_bus`'s NATS
//! suite.
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
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use af_core::id::Uuid7;
use afd_bus::test_util::Server;
use afd_bus::{Handle, Subject};
use afd_sse::frame::Frame;
use afd_sse::stream_limit::StreamLimit;
use afd_sse::tail::tail;
use afd_sse::{KIND_HELLO, Live};
use futures_util::StreamExt as _;

mod fan_in;

/// How long a frame may take to travel publisher → bus → tail before the test
/// fails rather than hangs. A healthy run never spends it.
const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// The fleet the single-fleet tests tail. Every test opens a bus of its own,
/// so no two tests share a subject.
const FLEET: &str = "0195b4ba-8d3a-7000-8abc-000000000001";

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

/// A bus of this test's own, with its server half.
///
/// The global subscriber makes every event macro on these paths evaluate its
/// fields, so the coverage lane counts the diagnostics they raise.
fn open_bus() -> (Handle, Server) {
    af_core::test_util::trace::install_subscriber();
    afd_bus::test_util::detached().expect("an in-process bus opens")
}

/// A fixture fleet's identifier.
fn fleet(id: &str) -> Uuid7 {
    Uuid7::parse(id).expect("a canonical fixture fleet")
}

/// Dimension 5.1 — frames arrive in publish order, numbered from zero, and a
/// reconnect starts over rather than resuming.
///
/// Three claims over one channel, because they are three facts about the SAME
/// stream:
///
/// 1. eight payloads published in order arrive in that order;
/// 2. their sequence numbers are `0..8` — gapless, and starting at zero;
/// 3. a second connection to the same channel numbers from zero AGAIN, and
///    receives nothing published while nobody held a subscription.
#[tokio::test(flavor = "multi_thread")]
async fn test_sse_sequencing_semantics() {
    let (bus, server) = open_bus();
    let activity = Subject::FleetTail(fleet(FLEET));

    assert_ordered_frames(&bus, &activity).await;
    assert_reconnect_starts_over(&bus, &server, &activity).await;
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
async fn test_fleet_stream_opens_with_hello_before_any_activity() {
    let (bus, _server) = open_bus();
    let fleet_id = fleet(FLEET);
    let activity = Subject::FleetTail(fleet_id.clone());

    let live = Live::new(bus.clone(), StreamLimit::new(1));
    let mut stream = Box::pin(live.tail_of(&fleet_id).await);

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
        hello.data.text().contains(FLEET),
        "the opening frame names the fleet it carries: {}",
        hello.data
    );
    assert_eq!(
        hello.seq, 0,
        "a control frame rides the synthetic sequence, never the connection's"
    );
    let catching_up = next_frame(&mut stream).await;
    assert_eq!(
        catching_up.data.text(),
        r#"{"kind":"catching_up","dropped":0}"#,
        "a fresh tail tells the client to backfill before live frames arrive"
    );
    assert_eq!(catching_up.seq, 0, "the catch-up is a control frame too");

    bus.publish(&activity, &payload(0))
        .await
        .expect("the publish reaches the bus");
    let first = next_frame(&mut stream).await;
    assert_eq!(
        first.seq, 0,
        "the hello spent no activity number -- the first real frame is still zero"
    );
    assert_eq!(first.data, payload(0));
}

async fn assert_ordered_frames(bus: &Handle, activity: &Subject) {
    let mut first = Box::pin(tail(
        bus.subscribe(activity).await.expect("the bus subscribes"),
    ));

    for n in 0..ORDERED_FRAMES {
        bus.publish(activity, &payload(n))
            .await
            .expect("the publish reaches the bus");
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

async fn assert_reconnect_starts_over(bus: &Handle, server: &Server, activity: &Subject) {
    assert_eq!(
        server.readers(activity),
        0,
        "the first connection's subscription closed with it"
    );
    let missed = r#"{"kind":"run_output","n":"during-the-gap"}"#;
    bus.publish(activity, missed)
        .await
        .expect("the publish reaches the bus");

    let mut second = Box::pin(tail(
        bus.subscribe(activity).await.expect("the bus subscribes"),
    ));
    let resumed = payload(0);
    bus.publish(activity, &resumed)
        .await
        .expect("the publish reaches the bus");

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

/// The next frame, or a failed test rather than a hung one.
async fn next_frame<S>(stream: &mut S) -> Frame
where
    S: futures_util::Stream<Item = Frame> + Unpin,
{
    tokio::time::timeout(DELIVERY_BUDGET, stream.next())
        .await
        .expect("a frame must arrive inside the delivery budget")
        .expect("the tail must not end while a frame is owed")
}
