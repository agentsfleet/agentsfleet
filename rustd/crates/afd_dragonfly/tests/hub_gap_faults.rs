//! Readers are told about every lost subscription, and a slow subscribe never
//! holds a frame.
//!
//! The fake is the service here, so these run in the fast lane. The live
//! cluster's half — one node's socket killed among four — is
//! `integration_hub_exclusive`, which has to run alone.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::hub::Received;
use afd_dragonfly::{Subscription, SubscriptionHub};
use backon::ExponentialBuilder;
use tokio::time::Instant;

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// Short enough that a hang fails the test rather than the lane's timeout.
const BUDGET: Duration = Duration::from_secs(10);

/// How long the held subscribe is held. Well inside the connection's
/// five-second reply deadline, so the hold is slow rather than failed.
const HOLD: Duration = Duration::from_secs(2);

/// How often a publish is retried while a reader waits for it.
const RETRY: Duration = Duration::from_millis(25);

const FIRST: &str = "fleet:first:activity";
const SECOND: &str = "fleet:second:activity";
const LATE: &str = "fleet:late:activity";

/// A redial schedule a test can wait out; see `hub_socket_faults::impatient`.
fn impatient() -> ExponentialBuilder {
    ExponentialBuilder::new()
        .with_min_delay(Duration::from_millis(5))
        .with_max_delay(Duration::from_millis(20))
        .without_max_times()
}

async fn fake_and_hub() -> (FakeRedis, SubscriptionHub) {
    install_subscriber();
    let fake = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("SSUBSCRIBE", Reply::SubscribeAck),
        ("SUNSUBSCRIBE", Reply::UnsubscribeAck),
    ])
    .await;
    let config = DragonflyConfig::from_url(DragonflyRole::Api, fake.url());
    let hub = SubscriptionHub::start_with_backoff(config, impatient())
        .await
        .expect("the fake accepts the first connection");
    (fake, hub)
}

/// Publishes `payload` until `reader` receives it, returning every gap it was
/// told about on the way.
async fn deliver(fake: &FakeRedis, channel: &str, payload: &str, reader: &mut Subscription) -> u32 {
    let deadline = Instant::now() + BUDGET;
    let mut gaps = 0;
    loop {
        fake.publish(channel, payload);
        match tokio::time::timeout(RETRY, reader.recv()).await {
            Ok(Ok(Received::Message(message))) if message.payload == payload => return gaps,
            Ok(Ok(Received::Gap)) => gaps += 1,
            Ok(Ok(Received::Message(_) | Received::Lagged(_))) | Err(_) => {}
            Ok(Err(closed)) => panic!("the reader's channel closed: {closed}"),
        }
        assert!(
            Instant::now() < deadline,
            "{payload} never reached {channel}"
        );
    }
}

/// Waits until `reader` is told its subscription was lost and is back.
async fn gap_on(reader: &mut Subscription) {
    let seen = tokio::time::timeout(BUDGET, async {
        loop {
            match reader.recv().await.expect("the hub stays open") {
                Received::Gap => return,
                Received::Message(_) | Received::Lagged(_) => {}
            }
        }
    })
    .await;
    assert!(
        seen.is_ok(),
        "{} was never told about its gap",
        reader.channel()
    );
}

/// Dimension 5.3: a subscribe held for two seconds leaves another channel's
/// frames flowing inside the hold.
#[tokio::test(flavor = "multi_thread")]
async fn test_dispatch_continues_during_slow_subscribe() {
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    deliver(&fake, FIRST, "primed", &mut first).await;

    fake.set_reply("SSUBSCRIBE", Reply::HeldSubscribeAck(HOLD));
    let _late = hub.subscribe(LATE);
    let asked = Instant::now() + BUDGET;
    while subscribes_seen(&fake) < 2 {
        assert!(
            Instant::now() < asked,
            "the second subscribe never reached the server"
        );
        tokio::time::sleep(RETRY).await;
    }

    // A quarter of the hold, not all of it: a pump that waited out the
    // subscribe would deliver this frame just as the hold ended, which a
    // bound of the whole hold could not tell from a pump that never waited.
    let held = Instant::now();
    fake.publish(FIRST, "during-the-hold");
    let received = tokio::time::timeout(HOLD / 4, first.recv())
        .await
        .expect("a frame waited on the held subscribe")
        .expect("the hub stays open");
    assert!(
        matches!(&received, Received::Message(message) if message.payload == "during-the-hold"),
        "the frame published during the hold, got {received:?}"
    );
    assert!(
        held.elapsed() < HOLD / 4,
        "delivered in {:?}",
        held.elapsed()
    );
    assert_eq!(
        hub.connections_opened(),
        1,
        "a slow subscribe is not a loss"
    );
}

/// Dimension 5.1: a whole-connection redial gaps every live channel, after
/// which frames resume on the same readers.
#[tokio::test(flavor = "multi_thread")]
async fn test_reconnect_sends_a_gap() {
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    let mut second = hub.subscribe(SECOND);
    deliver(&fake, FIRST, "primed", &mut first).await;
    deliver(&fake, SECOND, "primed", &mut second).await;

    // An unsubscribe the server answers by hanging up is a failed command,
    // which the control task can only answer with a redial.
    fake.set_reply("SUNSUBSCRIBE", Reply::Hangup);
    drop(hub.subscribe(LATE));
    let redialled = Instant::now() + BUDGET;
    while hub.connections_opened() < 2 {
        assert!(
            Instant::now() < redialled,
            "the failed command never redialled"
        );
        tokio::time::sleep(RETRY).await;
    }

    gap_on(&mut first).await;
    gap_on(&mut second).await;
    deliver(&fake, FIRST, "resumed", &mut first).await;
    deliver(&fake, SECOND, "resumed", &mut second).await;
}

/// The single-node half of Dimension 5.2: the server closing the socket is
/// repaired inside the connection the hub already has, and the hub's own
/// re-subscribe of every channel is the gap their readers are told about.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_repaired_node_gaps_its_channels_without_a_redial() {
    let (fake, hub) = fake_and_hub().await;
    let mut first = hub.subscribe(FIRST);
    let mut second = hub.subscribe(SECOND);
    deliver(&fake, FIRST, "primed", &mut first).await;
    deliver(&fake, SECOND, "primed", &mut second).await;
    let subscribed = subscribes_seen(&fake);

    fake.cut();
    gap_on(&mut first).await;
    gap_on(&mut second).await;
    // The driver may also replay on its own, which confirms again: a second
    // gap may follow. Over-reporting is allowed; silence and a redial are not.
    deliver(&fake, FIRST, "resumed", &mut first).await;
    deliver(&fake, SECOND, "resumed", &mut second).await;
    assert_eq!(
        hub.connections_opened(),
        1,
        "the node was repaired in place"
    );
    assert!(
        subscribes_seen(&fake) >= subscribed + 2,
        "the hub re-subscribed each channel with a command of its own"
    );
}

/// How many `SSUBSCRIBE`s the fake has been sent.
fn subscribes_seen(fake: &FakeRedis) -> usize {
    fake.seen()
        .iter()
        .filter(|name| *name == "SSUBSCRIBE")
        .count()
}
