//! Dimensions 3.2 and 3.3 — one connection for N readers, and what happens
//! when that connection dies.
//!
//! These two run one at a time. 3.3 kills pub/sub connections server-side, and
//! there is no way to kill only one hub's: a parallel 3.2 would see its own
//! connection dropped mid-assertion and fail for the wrong reason. Serialising
//! is honest about that; retrying would be hiding it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_redis::SubscriptionHub;
use afd_redis::hub::Received;
use afd_redis::streams::FleetStreams;
use backon::ExponentialBuilder;
use tokio::sync::Mutex;

use crate::support::RedisHarness;

/// Serialises the two hub tests. See the module documentation.
static HUB_LANE: Mutex<()> = Mutex::const_new(());

/// How long a message may take to travel publisher → Redis → hub → reader.
const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// Dimension 3.2 — N readers, one connection, and a channel that closes when
/// the last of them goes.
///
/// Invariant 2 of the milestone is "exactly one Redis subscribe connection per
/// process". The number that proves it is the hub's own connection count: a
/// hub that opened one connection per subscriber would report four here.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_hub_refcount_single_connection() {
    let _lane = HUB_LANE.lock().await;
    let harness = RedisHarness::connect().await;
    let publisher = FleetStreams::new(harness.redis.clone());
    let channel = harness.name("channel");

    let hub = SubscriptionHub::start(RedisHarness::config())
        .await
        .expect("hub starts");
    assert_eq!(hub.connections_opened(), 1, "the hub opens one connection");

    let mut readers = Vec::new();
    for _ in 0..4_u8 {
        readers.push(hub.subscribe(&channel));
    }
    assert_eq!(hub.readers(&channel), 4);
    assert_eq!(
        hub.connections_opened(),
        1,
        "four readers must share one connection, not open four"
    );

    // The server agrees: one subscriber on the channel, however many readers
    // this process has. Waited for rather than asserted outright — `subscribe`
    // queues the SUBSCRIBE and the pump issues it, so the registration is
    // asynchronous by construction. An immediate assertion passes on a fast
    // machine and fails on a loaded one, which is the definition of a flake.
    wait_for(|| async { server_subscriber_count(&harness, &channel).await == 1 }).await;

    // Every reader receives the same message.
    publish_until_delivered(&publisher, &channel, "hello", &mut readers[0]).await;
    for reader in &mut readers[1..] {
        let received = tokio::time::timeout(DELIVERY_BUDGET, reader.recv())
            .await
            .expect("delivery within budget")
            .expect("the hub is live");
        let Received::Message(message) = received else {
            panic!("a message, not a lag notice");
        };
        assert_eq!(message.payload, "hello");
        assert_eq!(message.channel, channel);
    }

    // Dropping all but one keeps the subscription; dropping the last releases it.
    readers.truncate(1);
    assert_eq!(hub.readers(&channel), 1);
    assert_eq!(
        server_subscriber_count(&harness, &channel).await,
        1,
        "a channel with readers left must stay subscribed"
    );
    assert_eq!(
        hub.connections_opened(),
        1,
        "dropping readers must not have cost a reconnect"
    );

    readers.clear();
    assert_eq!(hub.readers(&channel), 0, "the refcount must reach zero");
    wait_for(|| async { server_subscriber_count(&harness, &channel).await == 0 }).await;
}

/// Dimension 3.3 — the connection dies, the hub redials, and readers that never
/// noticed keep receiving.
///
/// What is NOT claimed: that messages published during the gap arrive. Pub/sub
/// has no replay, and a test that pretended otherwise would be encoding a
/// promise the transport cannot keep.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_hub_reconnect_resubscribes() {
    let _lane = HUB_LANE.lock().await;
    let harness = RedisHarness::connect().await;
    let publisher = FleetStreams::new(harness.redis.clone());
    let channel = harness.name("channel");

    // A short schedule: the production one would have this test waiting out a
    // fifth of a second per attempt for no extra proof.
    let hub = SubscriptionHub::start_with_backoff(
        RedisHarness::config(),
        ExponentialBuilder::new()
            .with_min_delay(Duration::from_millis(20))
            .with_max_delay(Duration::from_millis(100)),
    )
    .await
    .expect("hub starts");

    let mut reader = hub.subscribe(&channel);
    publish_until_delivered(&publisher, &channel, "before", &mut reader).await;
    assert_eq!(hub.connections_opened(), 1);

    // Kill it the way an operator, a failover or an idle timeout would.
    let mut kill = redis::cmd("CLIENT");
    kill.arg("KILL").arg("TYPE").arg("pubsub");
    let _: i64 = harness
        .redis
        .command("CLIENT", "kill", &kill)
        .await
        .expect("kill the pub/sub connections");

    // The hub notices, redials, and resubscribes what readers still hold.
    wait_for(|| async { hub.connections_opened() > 1 }).await;
    wait_for(|| async { server_subscriber_count(&harness, &channel).await == 1 }).await;

    // The reader never touched anything, and receives again.
    publish_until_delivered(&publisher, &channel, "after", &mut reader).await;
    assert_eq!(
        hub.readers(&channel),
        1,
        "the refcount survives a reconnect"
    );
}

/// Publishes until the reader sees the payload, or the budget runs out.
///
/// A subscription is registered asynchronously — the hub queues `SUBSCRIBE` and
/// the pump issues it — so a single publish immediately after `subscribe` can
/// land before the server has the subscription. Retrying the publish is the
/// deterministic fix; a sleep would be a guess that fails on a slow runner.
async fn publish_until_delivered(
    publisher: &FleetStreams,
    channel: &str,
    payload: &str,
    reader: &mut afd_redis::Subscription,
) {
    let deadline = tokio::time::Instant::now() + DELIVERY_BUDGET;
    loop {
        publisher
            .publish(channel, payload)
            .await
            .expect("publish succeeds");

        match tokio::time::timeout(Duration::from_millis(100), reader.recv()).await {
            Ok(Ok(Received::Message(message))) if message.payload == payload => return,
            Ok(Ok(_)) => {}
            Ok(Err(failure)) => panic!("the hub closed: {failure}"),
            Err(_elapsed) => {}
        }

        assert!(
            tokio::time::Instant::now() < deadline,
            "{payload:?} never reached the reader on {channel}"
        );
    }
}

/// How many subscribers the SERVER thinks the channel has.
async fn server_subscriber_count(harness: &RedisHarness, channel: &str) -> i64 {
    let mut cmd = redis::cmd("PUBSUB");
    cmd.arg("NUMSUB").arg(channel);
    let reply: Vec<redis::Value> = harness
        .redis
        .command("PUBSUB", channel, &cmd)
        .await
        .expect("PUBSUB NUMSUB");
    match reply.get(1) {
        Some(redis::Value::Int(count)) => *count,
        other => panic!("unexpected NUMSUB reply: {other:?}"),
    }
}

/// Waits for a condition, failing the test rather than the lane if it never
/// holds. Polling because the events being waited on happen on a server and in
/// another task, with no handshake to await.
async fn wait_for<F, Fut>(mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + DELIVERY_BUDGET;
    while tokio::time::Instant::now() < deadline {
        if condition().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the condition never held inside {DELIVERY_BUDGET:?}");
}

/// A reader that falls behind is TOLD, and a reader on a stopped hub is told
/// that too — neither waits forever on something that will not arrive.
///
/// The buffer is bounded on purpose: an unbounded one turns a single stalled
/// browser tab into the process's memory ceiling. Bounded means a slow reader
/// eventually misses messages, and the only honest thing to do is say so.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_a_lagging_reader_is_told_and_a_stopped_hub_closes() {
    let _lane = HUB_LANE.lock().await;
    let harness = RedisHarness::connect().await;
    let publisher = FleetStreams::new(harness.redis.clone());
    let channel = harness.name("channel");

    let hub = SubscriptionHub::start(RedisHarness::config())
        .await
        .expect("hub starts");
    let mut reader = hub.subscribe(&channel);
    assert_eq!(reader.channel(), channel, "a reader knows what it reads");

    // Make sure the subscription is live before flooding it.
    publish_until_delivered(&publisher, &channel, "primed", &mut reader).await;

    // Well past the buffer, with nothing reading.
    for sequence in 0..600_u16 {
        publisher
            .publish(&channel, &format!("flood-{sequence}"))
            .await
            .expect("publish");
    }

    // Somewhere in there the reader is told it lagged rather than handed a
    // message that silently skipped its predecessors.
    let mut lagged = false;
    for _ in 0..600_u16 {
        match tokio::time::timeout(Duration::from_millis(200), reader.recv()).await {
            Ok(Ok(Received::Lagged(missed))) => {
                assert!(missed > 0, "a lag notice reports how many were dropped");
                lagged = true;
                break;
            }
            Ok(Ok(Received::Message(_))) => {}
            Ok(Err(failure)) => panic!("the hub closed early: {failure}"),
            Err(_elapsed) => break,
        }
    }
    assert!(lagged, "a reader {} messages behind was never told", 600);

    // Stopping the hub closes what readers are waiting on. What is already
    // buffered is delivered first — a closed channel still hands over what it
    // holds, which is the right order: shutdown must not drop messages the
    // reader was entitled to.
    hub.shutdown();
    let mut closed = None;
    for _ in 0..1_000_u16 {
        match reader.recv().await {
            Ok(_) => {}
            Err(failure) => {
                closed = Some(failure);
                break;
            }
        }
    }
    let error = closed.expect("a reader on a stopped hub must be told, not parked");
    assert!(error.is_hub_closed(), "got {error}");
    assert_eq!(hub.readers(&channel), 0, "shutdown drops every channel");

    // Dropping a subscription whose channel is already gone is a no-op, not a
    // panic — shutdown and Drop race by construction.
    drop(reader);
}
