//! What the pub/sub pump does when its socket misbehaves.
//!
//! `integration_hub.rs` proves the hub recovers when Redis drops it and comes
//! back. These prove what happens on the way there, and on the paths where it
//! does not come back: a first connection that is refused outright, a redial
//! that keeps being refused, and a redial that SUCCEEDS onto a server which is
//! listening but not serving — the failover shape, where the port is bound
//! before anything behind it works.
//!
//! That last one is the case worth having a fixture for. A resubscribe that
//! fails silently leaves a reader holding a live `Subscription` on a channel
//! the server has never heard of: no error, no messages, forever. The hub logs
//! it and carries on to the next redial, which is the right behaviour, and this
//! is what holds it there.
//!
//! No live service — the fake is the service — so these run in the fast lane.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_redis::config::{RedisConfig, RedisRole};
use afd_redis::{Backoff, SubscriptionHub};

#[path = "support/fake_redis.rs"]
mod fake_redis;

use self::fake_redis::{FakeRedis, Reply, closed_port, install_subscriber};

/// Short enough that a hang fails the test rather than the lane's timeout.
const BUDGET: Duration = Duration::from_secs(10);

/// A redial schedule a test can wait out. Production doubles from 200ms to five
/// seconds, which no test should sit through.
const IMPATIENT: Backoff = Backoff::new(Duration::from_millis(5), Duration::from_millis(20));

/// The rule table for a fake that speaks enough pub/sub to hold a hub.
fn pubsub_rules() -> Vec<(&'static str, Reply)> {
    vec![
        ("PING", Reply::Raw("+PONG\r\n")),
        ("SUBSCRIBE", Reply::SubscribeAck),
        ("UNSUBSCRIBE", Reply::UnsubscribeAck),
    ]
}

/// Waits for `check` to hold, or gives up inside the budget.
///
/// Polling, not sleeping: the pump is a background task on its own schedule,
/// and a fixed sleep either makes the test slow or makes it flaky on a loaded
/// machine. The budget is the failure, not the wait.
async fn until(label: &str, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + BUDGET;
    while tokio::time::Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("timed out waiting for {label}");
}

/// A first connection that is refused fails the hub's start, rather than
/// returning a hub that reconnects forever behind a healthy-looking `/readyz`.
///
/// This is the difference between a process that refuses to boot and one that
/// boots into a state where every event stream is silently empty. The first is
/// a page at deploy time; the second is a bug report a week later.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_refused_first_connection_refuses_the_hub() {
    install_subscriber();
    let config = RedisConfig::from_url(RedisRole::Default, closed_port().await);

    let error = tokio::time::timeout(BUDGET, SubscriptionHub::start(config))
        .await
        .expect("a refused connection must fail fast, not hang")
        .expect_err("a hub that cannot reach Redis must not start");

    assert!(
        error.is_unavailable(),
        "a refused socket is an outage, and must name it: {error}"
    );
    assert!(
        error.to_string().contains("default"),
        "the failure must name the role, since a deployment runs two: {error}"
    );
}

/// A dropped socket that cannot be redialled leaves the hub retrying, not
/// counting connections it never opened.
///
/// The count is the assertion that matters: `connections_opened` is what
/// Invariant 2 is measured by, and a pump that incremented it on a failed
/// redial would make a broken hub look like a healthy one.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_redial_that_keeps_being_refused_never_counts_a_connection() {
    install_subscriber();
    let server = FakeRedis::spawn(&pubsub_rules()).await;
    let config = RedisConfig::from_url(RedisRole::Default, server.url());

    let hub = tokio::time::timeout(
        BUDGET,
        SubscriptionHub::start_with_backoff(config, IMPATIENT),
    )
    .await
    .expect("the fake serves, so the hub must start")
    .expect("the fake serves, so the hub must start");
    let subscription = hub.subscribe("channel");
    until("the first SUBSCRIBE to reach the server", || {
        server.seen().iter().any(|command| command == "SUBSCRIBE")
    })
    .await;
    assert_eq!(hub.connections_opened(), 1, "the hub opens one connection");

    // The server goes away entirely: the port is freed first, so the redial
    // that follows the cut is refused rather than racing the shutdown.
    server.stop_listening();
    server.cut();

    // Long enough for many redials at this backoff. Every one of them fails.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        hub.connections_opened(),
        1,
        "a refused redial must not be counted as a connection"
    );

    drop(subscription);
}

/// A redial that succeeds onto a server which has stopped honouring
/// `SUBSCRIBE` re-counts the connection and survives the failed resubscribe.
///
/// This is the failover shape: the port is bound and the handshake completes,
/// so the redial genuinely succeeds, and only the resubscribe finds out that
/// nothing behind it works. The pump must not treat that as fatal — the next
/// loop discovers the dead socket and the redial after it is the one that
/// recovers. Aborting here would turn a transient failover into a hub that
/// never comes back, with every reader still holding a live `Subscription` on
/// a channel the server has never heard of.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_resubscribe_onto_a_dead_socket_is_logged_and_survived() {
    install_subscriber();
    let server = FakeRedis::spawn(&pubsub_rules()).await;
    let config = RedisConfig::from_url(RedisRole::Default, server.url());

    let hub = tokio::time::timeout(
        BUDGET,
        SubscriptionHub::start_with_backoff(config, IMPATIENT),
    )
    .await
    .expect("the fake serves, so the hub must start")
    .expect("the fake serves, so the hub must start");
    let subscription = hub.subscribe("channel");
    until("the first SUBSCRIBE to reach the server", || {
        server.seen().iter().any(|command| command == "SUBSCRIBE")
    })
    .await;

    // From here the server answers everything EXCEPT `SUBSCRIBE`, which it
    // hangs up on. The redial completes its handshake and is counted; the
    // resubscribe that follows it is the thing that fails.
    server.set_reply("SUBSCRIBE", Reply::Hangup);
    server.cut();

    until("the pump to redial onto the half-working server", || {
        hub.connections_opened() >= 2
    })
    .await;

    // Still pumping: it keeps redialling rather than giving up on the channel.
    let before = hub.connections_opened();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        hub.connections_opened() > before,
        "the pump must keep redialling after a failed resubscribe"
    );

    drop(subscription);
}

/// The last reader leaving a channel over a socket that dies on the way out is
/// a dropped connection, not a lost unsubscribe.
///
/// The pump learns its socket is dead in two places — the message stream ending
/// and a command it issues failing — and this is the second one. It matters
/// because the command arm is where the hub's OWN bookkeeping is: the channel
/// has already been removed from the map by the time the write fails, so a pump
/// that swallowed the error would carry on believing it holds a subscription
/// the server closed underneath it, and the resubscribe after the next redial
/// would never re-establish it.
///
/// Deterministic despite the `select!`: the `Unsubscribe` is queued before the
/// socket has anything to say, so the command arm is the ready one, and its
/// handler then runs to completion rather than racing the stream.
#[tokio::test(flavor = "multi_thread")]
async fn test_an_unsubscribe_over_a_dying_socket_is_a_dropped_connection() {
    install_subscriber();
    let server = FakeRedis::spawn(&pubsub_rules()).await;
    let config = RedisConfig::from_url(RedisRole::Default, server.url());

    let hub = tokio::time::timeout(
        BUDGET,
        SubscriptionHub::start_with_backoff(config, IMPATIENT),
    )
    .await
    .expect("the fake serves, so the hub must start")
    .expect("the fake serves, so the hub must start");
    let subscription = hub.subscribe("channel");
    until("the first SUBSCRIBE to reach the server", || {
        server.seen().iter().any(|command| command == "SUBSCRIBE")
    })
    .await;
    assert_eq!(hub.connections_opened(), 1, "the hub opens one connection");

    // The socket stays up and quiet until the unsubscribe arrives, and dies on
    // that command specifically — so the pump meets the failure through the
    // command it issued rather than through the stream ending under it.
    server.set_reply("UNSUBSCRIBE", Reply::Hangup);
    drop(subscription);

    // The redial is the proof: reaching it means the pump treated the failed
    // unsubscribe as a dropped connection instead of ignoring it.
    until("the pump to redial after the failed unsubscribe", || {
        hub.connections_opened() >= 2
    })
    .await;
    assert_eq!(
        hub.readers("channel"),
        0,
        "the last reader left, so nothing may be resubscribed on the new socket"
    );
}

/// The last handle going away stops the pump and closes the Redis socket.
///
/// This is Invariant C2's precondition — §7 cannot join a task that has no way
/// to finish — and it is a claim about ownership, not about politeness. The
/// pump holds an `Arc<HubInner>` for as long as it runs, so if the command
/// sender lived in `HubInner` the pump would be keeping its own wake-up signal
/// alive: `recv()` could never return `None`, the task would pump a live socket
/// forever, and a process could accumulate one such task per hub it ever built.
/// The sender therefore lives on the handles instead, and this is what holds it
/// there.
///
/// Counted server-side on purpose. A client-side assertion could only say the
/// hub stopped being used; only the server can say the socket actually closed.
#[tokio::test(flavor = "multi_thread")]
async fn test_dropping_every_handle_stops_the_pump_and_closes_the_socket() {
    install_subscriber();
    let server = FakeRedis::spawn(&pubsub_rules()).await;
    let config = RedisConfig::from_url(RedisRole::Default, server.url());

    let hub = tokio::time::timeout(
        BUDGET,
        SubscriptionHub::start_with_backoff(config, IMPATIENT),
    )
    .await
    .expect("the fake serves, so the hub must start")
    .expect("the fake serves, so the hub must start");
    let subscription = hub.subscribe("channel");
    until("the hub's connection to reach the server", || {
        server.live_connections() == 1
    })
    .await;

    // A reader outliving the hub keeps the pump running: it still holds a
    // sender, and its subscription is still live. Dropping the hub alone must
    // NOT take the socket down underneath it.
    drop(hub);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        server.live_connections(),
        1,
        "a live reader must keep the connection it is reading from"
    );

    // The last handle. Now there is nothing left to serve.
    drop(subscription);
    until("the pump to close its socket once nothing holds it", || {
        server.live_connections() == 0
    })
    .await;
}
