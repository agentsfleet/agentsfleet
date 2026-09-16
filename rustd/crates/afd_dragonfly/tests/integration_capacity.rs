//! Dimension 3.3 — the capacity sample accounts for streams, pending entries,
//! readiness partitions and replicas SEPARATELY, and counts each from the
//! datastore rather than inferring one from another.
//!
//! The lane's keyspace is shared with every other suite in this binary, and
//! with the suites in other crates' binaries, so each seeded figure is a lower
//! bound over what this test put there: another test's streams can only raise
//! a count, never lower it. The readiness figures carry upper bounds too,
//! chosen so that arriving marks cannot break them.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_dragonfly::streams::{FleetStreams, fleet_stream_key};
use afd_dragonfly::{Capacity, Partition, ReadyIndex};

use crate::support::DragonflyHarness;

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
    let harness = DragonflyHarness::connect().await;
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
    assert!(
        sample.ready_partitions >= 1,
        "the marked fleet's partition is counted: {sample:?}"
    );
    let declared = u64::try_from(Partition::all().count()).expect("fits");
    assert!(
        sample.ready_partitions <= declared,
        "a count of occupied partitions can never exceed the width the index \
         declares: {sample:?}"
    );
    // The discriminator this Dimension exists for, and the one an exact figure
    // cannot give here. Counting the declared width instead of the occupied
    // partitions would report sixteen while fewer than sixteen marks are held,
    // so the count must never outrun the marks it is drawn from. Unlike a
    // pinned figure this survives a contaminated lane: `READY_INDEX_KEY` is one
    // global key, the readiness tests in OTHER crates are other test binaries
    // and so other processes, and no mutex in this binary can keep them out.
    // Contamination only ever adds marks, which this bound tolerates.
    assert!(
        sample.ready_partitions <= sample.ready_marks,
        "the sample counts OCCUPIED partitions rather than the width the index \
         declares, so it can never exceed the marks held across them: {sample:?}"
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
    // `>=` and not `==`, for the reason the readiness bound above already
    // gives. `streams` is a SCAN over a global glob, so it counts every fleet
    // stream in the datastore, including ones other tests in this tier created
    // between the two samples. The regression worth catching is a cap that
    // TRUNCATES the count -- that shows up as capped < sample, and this
    // catches it. Equality additionally demanded that nothing else in a
    // parallel lane wrote a stream, which is not a property of the code under
    // test, and it failed exactly that way: green one run, red the next, no
    // change in between.
    assert!(
        capped.streams >= sample.streams,
        "the cap does not hide streams: capped {} < full {}",
        capped.streams,
        sample.streams
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
