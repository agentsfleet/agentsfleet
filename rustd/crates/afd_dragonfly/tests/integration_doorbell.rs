// Value: protects=sharded notifications reach every process's held beats;
// fails_when=publish routing or listener delivery breaks; why_new=lost-wake
// tests prove fallback only; seam=none; no_claim=durable queue correctness.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "failed fixture setup must fail the test"
)]

use std::time::Duration;

use afd_dragonfly::doorbell::{self, Doorbell, WAKE_SHARDS, Waited};
use afd_dragonfly::{FleetStreams, SubscriptionHub};

use crate::support::DragonflyHarness;

const BUDGET: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn sharded_notifications_reach_each_process_and_shutdown_releases_them() {
    let harness = DragonflyHarness::connect().await;
    let first = SubscriptionHub::start(DragonflyHarness::config())
        .await
        .expect("first hub");
    let second = SubscriptionHub::start(DragonflyHarness::config())
        .await
        .expect("second hub");
    let bells = [Doorbell::new(), Doorbell::new()];
    let [first_bell, second_bell] = &bells;
    first_bell.listen(&first);
    second_bell.listen(&second);
    let channels: Vec<_> = (0..WAKE_SHARDS)
        .map(|shard| format!("fleet:wake:{{{shard}}}"))
        .collect();
    subscriptions(&harness, &channels, 2).await;
    FleetStreams::new(harness.redis.clone())
        .publish("fleet:wake:{0}", "malformed fleet")
        .await
        .expect("publish malformed ring");
    // Fleets spread over the shards: each ring must reach a held beat on both
    // processes. A malformed payload rings nobody, so it leaves no permit for
    // the first wait to find.
    for index in 0..24 {
        let fleet = format!("0195b4ba-8d3a-7000-8abc-{index:012x}");
        rung_on_both(&bells, &harness, &fleet).await;
    }
    assert_eq!(first.connections_opened(), 1);
    assert_eq!(second.connections_opened(), 1);
    first.shutdown();
    second.shutdown();
    drop((first, second));
    subscriptions(&harness, &channels, 0).await;
}

/// Holds one beat on each process, publishes one ring for `fleet`, and
/// expects both beats to answer `Rung` well before their deadline.
async fn rung_on_both(bells: &[Doorbell; 2], harness: &DragonflyHarness, fleet: &str) {
    let [first, second] = bells.clone();
    let waits = [
        tokio::spawn(async move { first.wait_for_ring(BUDGET, 1).await }),
        tokio::spawn(async move { second.wait_for_ring(BUDGET, 1).await }),
    ];
    doorbell::publish(&harness.redis, fleet).await;
    for wait in waits {
        assert_eq!(
            wait.await.expect("the held beat runs to its answer"),
            Waited::Rung,
            "{fleet}'s ring reaches each process"
        );
    }
}

async fn subscriptions(harness: &DragonflyHarness, channels: &[String], wanted: i64) {
    let mut counts = Vec::new();
    let ready = tokio::time::timeout(BUDGET, async {
        loop {
            counts.clear();
            for channel in channels {
                counts.push(subscriber_count(harness, channel).await);
            }
            if counts.iter().all(|count| *count == wanted) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(ready.is_ok(), "expected {wanted} subscribers: {counts:?}");
}

async fn subscriber_count(harness: &DragonflyHarness, channel: &str) -> i64 {
    let mut command = redis::cmd("PUBSUB");
    command.arg("NUMSUB").arg(channel);
    let reply: Vec<redis::Value> = harness
        .redis
        .command("PUBSUB", channel, &command)
        .await
        .expect("server subscriber count");
    reply
        .into_iter()
        .filter_map(|value| match value {
            redis::Value::Map(entries) => Some(entries),
            _ => None,
        })
        .flatten()
        .filter_map(|(_, value)| match value {
            redis::Value::Int(count) => Some(count),
            _ => None,
        })
        .sum()
}
