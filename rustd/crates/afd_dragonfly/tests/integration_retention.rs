//! Dimension 3.1 — a slow consumer under trim pressure keeps every pending
//! entry recoverable while acknowledged history stays bounded.
//!
//! The floor is asserted from the stream's own state after the trim, never
//! from the trim's answer: a trim that reported "kept everything owed" and
//! had not would pass an assertion on its return value, and the entries are
//! the only witness.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_dragonfly::streams::{ACKNOWLEDGED_HISTORY, EventId, FleetStreams, fleet_stream_key};

use crate::support::RedisHarness;

/// Entries appended above the history bound, so the trim has something to
/// remove and something it must not.
const ABOVE_THE_BOUND: usize = 50;

/// How many of the appended entries a consumer is handed, and how many of
/// those it acknowledges: the rest are pending, and everything never handed
/// out is undelivered.
const DELIVERED: usize = 30;
const ACKNOWLEDGED: usize = 20;

/// Appends `count` entries and answers their receipts, in order.
async fn append_many(streams: &FleetStreams, fleet: &str, count: usize) -> Vec<EventId> {
    let mut receipts = Vec::with_capacity(count);
    for index in 0..count {
        let sequence = index.to_string();
        receipts.push(
            streams
                .append(fleet, &[("sequence", sequence.as_str())])
                .await
                .expect("append"),
        );
    }
    receipts
}

/// Whether the stream still holds the entry with `receipt`.
async fn holds(harness: &RedisHarness, key: &str, receipt: &EventId) -> bool {
    let mut cmd = redis::cmd("XRANGE");
    cmd.arg(key).arg(receipt.as_str()).arg(receipt.as_str());
    let found: Vec<(String, Vec<String>)> = harness
        .redis
        .command("XRANGE", key, &cmd)
        .await
        .expect("the stream answers a range read");
    !found.is_empty()
}

/// Dimension 3.1.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_slow_consumer_keeps_every_owed_entry_while_history_stays_bounded() {
    let harness = RedisHarness::connect().await;
    let streams = FleetStreams::new(harness.redis.clone());
    let fleet = harness.name("fleet");
    let consumer = harness.name("consumer");
    let key = fleet_stream_key(&fleet);

    streams.ensure_group(&fleet).await.expect("group create");
    let receipts = append_many(&streams, &fleet, ACKNOWLEDGED_HISTORY + ABOVE_THE_BOUND).await;
    for expected in receipts.iter().take(DELIVERED) {
        let event = streams
            .read_new(&fleet, &consumer)
            .await
            .expect("read")
            .expect("delivered in order");
        assert_eq!(&event.receipt, expected);
    }
    for receipt in receipts.iter().take(ACKNOWLEDGED) {
        assert!(streams.ack(&fleet, receipt).await.expect("ack"));
    }

    let trimmed = streams.trim(&fleet).await.expect("trim");

    // Every owed entry survives: the pending ten and the undelivered rest.
    let oldest_pending = receipts.get(ACKNOWLEDGED).expect("a pending entry exists");
    assert!(
        holds(&harness, &key, oldest_pending).await,
        "the oldest pending entry is the floor and must survive the trim"
    );
    let first_undelivered = receipts
        .get(DELIVERED)
        .expect("an undelivered entry exists");
    assert!(
        holds(&harness, &key, first_undelivered).await,
        "an entry no consumer was handed must survive the trim"
    );
    let backlog = streams
        .backlog(&fleet)
        .await
        .expect("the group answers")
        .expect("the group exists");
    assert_eq!(
        backlog.pending,
        u64::try_from(DELIVERED - ACKNOWLEDGED).expect("fits"),
        "the trim must not touch the pending list"
    );
    assert_eq!(
        backlog.undelivered,
        Some(u64::try_from(ACKNOWLEDGED_HISTORY + ABOVE_THE_BOUND - DELIVERED).expect("fits")),
        "the trim must not touch undelivered entries"
    );
    // And the trim did remove acknowledged history: the floor was the oldest
    // pending entry, so everything acknowledged before it went.
    assert_eq!(
        trimmed.removed,
        u64::try_from(ACKNOWLEDGED).expect("fits"),
        "only acknowledged entries below the floor are removed"
    );
    assert!(
        !holds(&harness, &key, receipts.first().expect("the first receipt")).await,
        "an acknowledged entry below the floor is gone"
    );

    // Drain everything, and the bound is the only thing left holding entries.
    for receipt in receipts
        .iter()
        .skip(ACKNOWLEDGED)
        .take(DELIVERED - ACKNOWLEDGED)
    {
        assert!(streams.ack(&fleet, receipt).await.expect("ack"));
    }
    while let Some(event) = streams.read_new(&fleet, &consumer).await.expect("read") {
        assert!(streams.ack(&fleet, &event.receipt).await.expect("ack"));
    }
    let bounded = streams.trim(&fleet).await.expect("trim");
    assert_eq!(
        bounded.retained,
        u64::try_from(ACKNOWLEDGED_HISTORY).expect("fits"),
        "with nothing owed, acknowledged history keeps exactly its bound"
    );

    let mut del = redis::cmd("DEL");
    del.arg(&key);
    let _: i64 = harness
        .redis
        .command("DEL", &key, &del)
        .await
        .expect("cleanup");
}

/// A stream nobody has read is never trimmed: with no group, every entry is
/// owed, and a trim that removed any would be the silent loss the floor
/// exists to prevent.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stream_no_consumer_has_read_is_never_trimmed() {
    let harness = RedisHarness::connect().await;
    let streams = FleetStreams::new(harness.redis.clone());
    let fleet = harness.name("unread");
    let key = fleet_stream_key(&fleet);

    let receipts = append_many(&streams, &fleet, ACKNOWLEDGED_HISTORY + ABOVE_THE_BOUND).await;
    let trimmed = streams.trim(&fleet).await.expect("trim");
    assert_eq!(trimmed.removed, 0, "nothing is owed less than everything");
    assert!(
        holds(&harness, &key, receipts.first().expect("the first receipt")).await,
        "the oldest unread entry survives"
    );

    let mut del = redis::cmd("DEL");
    del.arg(&key);
    let _: i64 = harness
        .redis
        .command("DEL", &key, &del)
        .await
        .expect("cleanup");
}
