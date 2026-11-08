//! Outbound retention preserves pending and undelivered work.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::support::DragonflyHarness;
use afd_dragonfly::streams::{ACKNOWLEDGED_HISTORY, EventId, TRIM_SLACK};
use afd_dragonfly::{
    Dragonfly, OUTBOUND_CONSUMER_GROUP, OUTBOUND_STREAM_KEY, OutboundJob, OutboundQueue,
};

async fn isolated() -> (Dragonfly, OutboundQueue) {
    let harness = DragonflyHarness::connect().await;
    let config = DragonflyHarness::config()
        .with_namespace(&harness.name("retention"))
        .expect("namespace");
    let redis = Dragonfly::connect(&config).await.expect("connection");
    let queue = OutboundQueue::new(redis.clone());
    queue.ensure_group().await.expect("group");
    (redis, queue)
}

async fn append_many(queue: &OutboundQueue, count: usize) -> Vec<EventId> {
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        ids.push(
            queue
                .enqueue(OutboundJob {
                    provider: "fixture",
                    destination: "destination",
                    workspace_id: "workspace",
                    fleet_id: "fleet",
                    event_id: &index.to_string(),
                    answer: "answer",
                })
                .await
                .expect("enqueue"),
        );
    }
    ids
}

async fn deliver(redis: &Dragonfly, count: usize) -> Vec<String> {
    let mut command = redis::cmd("XREADGROUP");
    command
        .arg("GROUP")
        .arg(OUTBOUND_CONSUMER_GROUP)
        .arg("retention-reader")
        .arg("COUNT")
        .arg(count)
        .arg("STREAMS")
        .arg(redis.physical_key(OUTBOUND_STREAM_KEY))
        .arg(">");
    let reply: redis::streams::StreamReadReply = redis
        .command("XREADGROUP", OUTBOUND_STREAM_KEY, &command)
        .await
        .expect("deliver");
    reply
        .keys
        .into_iter()
        .flat_map(|key| key.ids.into_iter().map(|entry| entry.id))
        .collect()
}

async fn retained(redis: &Dragonfly) -> Vec<String> {
    let mut command = redis::cmd("XRANGE");
    command
        .arg(redis.physical_key(OUTBOUND_STREAM_KEY))
        .arg("-")
        .arg("+");
    let reply: redis::streams::StreamRangeReply = redis
        .command("XRANGE", OUTBOUND_STREAM_KEY, &command)
        .await
        .expect("retained entries");
    reply.ids.into_iter().map(|entry| entry.id).collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn slow_consumer_keeps_every_owed_entry_then_bounds_acknowledged_history() {
    let (redis, queue) = isolated().await;
    let count = ACKNOWLEDGED_HISTORY + TRIM_SLACK + 50;
    let ids = append_many(&queue, count).await;
    let delivered = deliver(&redis, 30).await;
    assert_eq!(
        delivered,
        ids.iter()
            .take(30)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    for id in ids.iter().take(20) {
        assert!(queue.ack(id).await.expect("ack"));
    }
    queue.trim().await.expect("trim with pending work");
    let after = retained(&redis).await;
    assert_eq!(
        after,
        ids.iter()
            .skip(20)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    // Acknowledge all delivered entries and deliver the untouched tail.
    for id in ids.iter().take(30).skip(20) {
        assert!(queue.ack(id).await.expect("ack"));
    }
    assert_eq!(deliver(&redis, count).await.len(), count - 30);
    for id in ids.iter().skip(30) {
        assert!(queue.ack(id).await.expect("ack"));
    }
    queue.trim().await.expect("trim acknowledged history");
    assert_eq!(
        retained(&redis).await,
        ids.iter()
            .skip(count - ACKNOWLEDGED_HISTORY)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    redis.spend_key(OUTBOUND_STREAM_KEY).await.expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_entirely_undelivered_backlog_is_never_trimmed() {
    let (redis, queue) = isolated().await;
    let ids = append_many(&queue, ACKNOWLEDGED_HISTORY + TRIM_SLACK + 1).await;
    queue.trim().await.expect("trim");
    assert_eq!(
        retained(&redis).await,
        ids.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
    redis.spend_key(OUTBOUND_STREAM_KEY).await.expect("cleanup");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_missing_stream_and_history_inside_slack_require_no_trim() {
    let (redis, queue) = isolated().await;
    redis
        .spend_key(OUTBOUND_STREAM_KEY)
        .await
        .expect("remove group and stream");
    queue.trim().await.expect("missing stream");
    queue.ensure_group().await.expect("recreate group");
    let ids = append_many(&queue, ACKNOWLEDGED_HISTORY).await;
    assert_eq!(deliver(&redis, ids.len()).await.len(), ids.len());
    for id in &ids {
        assert!(queue.ack(id).await.expect("ack"));
    }
    queue.trim().await.expect("inside slack");
    assert_eq!(retained(&redis).await.len(), ids.len());
    redis.spend_key(OUTBOUND_STREAM_KEY).await.expect("cleanup");
}
