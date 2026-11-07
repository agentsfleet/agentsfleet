//! Outbound capacity reports retained and pending entries separately.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::support::DragonflyHarness;
use afd_dragonfly::{
    Capacity, Dedicated, Dragonfly, OUTBOUND_STREAM_KEY, OutboundJob, OutboundQueue, OutboundReader,
};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn capacity_counts_outbound_state_and_reports_a_capped_walk() {
    let harness = DragonflyHarness::connect().await;
    let namespace = harness.name("capacity");
    let config = DragonflyHarness::config()
        .with_namespace(&namespace)
        .expect("namespace");
    let redis = Dragonfly::connect(&config).await.expect("connection");
    let queue = OutboundQueue::new(redis.clone());
    queue.ensure_group().await.expect("group");
    for _ in 0..5 {
        queue
            .enqueue(OutboundJob {
                provider: "fixture",
                destination: "destination",
                workspace_id: "workspace",
                fleet_id: "fleet",
                event_id: "event",
                answer: "answer",
            })
            .await
            .expect("enqueue");
    }
    let connection = Dedicated::connect(&config, Duration::from_millis(10))
        .await
        .expect("reader");
    let mut reader = OutboundReader::new(connection, namespace);
    for _ in 0..2 {
        reader
            .read_blocking(Duration::from_millis(10))
            .await
            .expect("read")
            .expect("delivery");
    }
    let sample = Capacity::sample(&redis, 1).await.expect("sample");
    assert_eq!((sample.streams, sample.streams_walked), (1, 1));
    assert_eq!((sample.retained_entries, sample.pending_entries), (5, 2));
    assert!(sample.primaries >= 1 && sample.replicas >= 1, "{sample:?}");
    let capped = Capacity::sample(&redis, 0).await.expect("capped sample");
    assert_eq!((capped.streams, capped.streams_walked), (1, 0));
    assert_eq!((capped.retained_entries, capped.pending_entries), (0, 0));
    drop(reader);
    redis.spend_key(OUTBOUND_STREAM_KEY).await.expect("cleanup");
}
