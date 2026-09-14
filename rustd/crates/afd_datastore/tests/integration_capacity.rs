//! Dimension 3.3 — the capacity sample accounts for streams, pending entries,
//! readiness partitions and replicas SEPARATELY, and counts each from the
//! datastore rather than inferring one from another.
//!
//! The lane's keyspace is shared with every other suite in this binary, so
//! every assertion is a lower bound over what this test seeded: another
//! test's streams can only raise a count, never lower it.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_datastore::streams::{FleetStreams, fleet_stream_key};
use afd_datastore::{Capacity, ReadyIndex};

use crate::support::RedisHarness;

/// Streams this test seeds, entries per stream, and how many of each stream's
/// entries are delivered and left pending.
const STREAMS: usize = 3;
const ENTRIES_EACH: usize = 5;
const PENDING_EACH: usize = 2;

/// Wide enough to describe every stream on the lane; the cap is proven
/// separately below.
const WALK_EVERYTHING: usize = 100_000;

/// Dimension 3.3.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_capacity_sample_accounts_for_every_class_of_retained_state_separately() {
    let harness = RedisHarness::connect().await;
    let streams = FleetStreams::new(harness.redis.clone());
    let consumer = harness.name("consumer");
    let mut fleets = Vec::with_capacity(STREAMS);
    for index in 0..STREAMS {
        let fleet = harness.name(&format!("fleet{index}"));
        streams.ensure_group(&fleet).await.expect("group create");
        for entry in 0..ENTRIES_EACH {
            let sequence = entry.to_string();
            streams
                .append(&fleet, &[("sequence", sequence.as_str())])
                .await
                .expect("append");
        }
        for _delivered in 0..PENDING_EACH {
            streams
                .read_new(&fleet, &consumer)
                .await
                .expect("read")
                .expect("delivered");
        }
        fleets.push(fleet);
    }
    let index = ReadyIndex::new(harness.redis.clone());
    let marked = fleets.first().expect("a fleet was seeded");
    index.mark(marked, marked).await.expect("mark ready");

    let sample = Capacity::sample(&harness.redis, WALK_EVERYTHING)
        .await
        .expect("the datastore answers");

    let seeded = u64::try_from(STREAMS).expect("fits");
    assert!(sample.streams >= seeded, "{sample:?} misses seeded streams");
    assert_eq!(
        sample.streams_walked, sample.streams,
        "an uncapped walk describes every stream"
    );
    assert!(
        sample.retained_entries >= u64::try_from(STREAMS * ENTRIES_EACH).expect("fits"),
        "{sample:?} misses retained entries"
    );
    assert!(
        sample.pending_entries >= u64::try_from(STREAMS * PENDING_EACH).expect("fits"),
        "{sample:?} misses pending entries"
    );
    assert!(
        sample.pending_entries < sample.retained_entries,
        "pending is a separate, smaller figure than retained: {sample:?}"
    );
    assert_eq!(
        sample.ready_partitions, 1,
        "ONE fleet was marked, so one partition holds a mark — the figure counts \
         occupied partitions, not the sixteen the index declares"
    );
    assert!(
        sample.ready_marks >= 1,
        "{sample:?} misses the readiness mark"
    );
    assert!(sample.primaries >= 1, "{sample:?} names no primary");
    assert!(
        sample.replicas >= 1,
        "the lane's cluster runs replicas and the sample must count them: {sample:?}"
    );

    // A capped walk says so: fewer described than found, never a smaller
    // deployment.
    let capped = Capacity::sample(&harness.redis, 1)
        .await
        .expect("the datastore answers");
    assert_eq!(
        capped.streams, sample.streams,
        "the cap does not hide streams"
    );
    assert_eq!(capped.streams_walked, 1, "the cap bounds what is described");

    index.force_clear(marked).await.expect("clear the mark");
    for fleet in &fleets {
        let key = fleet_stream_key(fleet);
        let mut del = redis::cmd("DEL");
        del.arg(&key);
        let _: i64 = harness
            .redis
            .command("DEL", &key, &del)
            .await
            .expect("cleanup");
    }
}
