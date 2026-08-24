//! Dimension 3.1 — append, read as a group, acknowledge; plus the readiness
//! index's token rule, which is the part of it that is easy to get wrong.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_redis::ready::ReadyIndex;
use afd_redis::streams::{FleetStreams, fleet_stream_key};

#[path = "support/redis_harness.rs"]
mod support;

use self::support::RedisHarness;

/// Dimension 3.1 — the round trip, and the identity claim inside it.
///
/// The claim that matters is not that a message survives the trip: it is that
/// the id Redis minted on append is the id the reader sees and acknowledges.
/// A second identifier anywhere in that chain is how an event gets processed
/// twice under two names.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_stream_xadd_readgroup_ack() {
    let harness = RedisHarness::connect().await;
    let streams = FleetStreams::new(harness.redis.clone());
    let fleet = harness.name("fleet");
    let consumer = harness.name("consumer");

    streams.ensure_group(&fleet).await.expect("group create");
    // Idempotent: the write path calls it once per fleet and a retry must not
    // be a failure.
    streams
        .ensure_group(&fleet)
        .await
        .expect("group create is idempotent");

    let appended = streams
        .append(&fleet, &[("type", "message"), ("actor", "user_1")])
        .await
        .expect("append");

    let event = streams
        .read_new(&fleet, &consumer)
        .await
        .expect("read")
        .expect("the appended event must be delivered");

    assert_eq!(
        event.id, appended,
        "the entry id IS the event id — there is no second identifier"
    );
    assert_eq!(event.field("type"), Some("message"));
    assert_eq!(event.field("actor"), Some("user_1"));

    // Delivered but unacknowledged, so it is this consumer's pending entry —
    // which is what a re-poll after a crash has to find.
    let pending = streams
        .read_pending(&fleet, &consumer)
        .await
        .expect("pending read")
        .expect("an unacknowledged event is pending");
    assert_eq!(pending.id, appended);

    assert!(streams.ack(&fleet, &appended).await.expect("ack"));
    assert!(
        streams
            .read_pending(&fleet, &consumer)
            .await
            .expect("pending read")
            .is_none(),
        "an acknowledged event must leave the pending list"
    );
    assert!(
        streams
            .read_new(&fleet, &consumer)
            .await
            .expect("read")
            .is_none(),
        "an acknowledged event must not be delivered again"
    );

    cleanup(&harness, &[fleet_stream_key(&fleet)]).await;
}

/// A stream whose consumer group vanished repairs itself on the next read,
/// once, and does not re-deliver its history.
///
/// The history part is the expensive half: a group recreated at the stream's
/// beginning hands out every retained entry again, and those are agent runs
/// that already spent real money.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_stream_repairs_a_missing_group_without_replaying_history() {
    let harness = RedisHarness::connect().await;
    let streams = FleetStreams::new(harness.redis.clone());
    let fleet = harness.name("fleet");
    let consumer = harness.name("consumer");
    let key = fleet_stream_key(&fleet);

    streams.ensure_group(&fleet).await.expect("group create");
    let historical = streams
        .append(&fleet, &[("type", "already_processed")])
        .await
        .expect("append");
    let delivered = streams
        .read_new(&fleet, &consumer)
        .await
        .expect("read")
        .expect("delivered");
    assert_eq!(delivered.id, historical);
    streams.ack(&fleet, &historical).await.expect("ack");

    // The group goes away — a restart without persistence, a failover, or an
    // operator with XGROUP DESTROY.
    let mut destroy = redis::cmd("XGROUP");
    destroy
        .arg("DESTROY")
        .arg(&key)
        .arg(afd_redis::streams::FLEET_CONSUMER_GROUP);
    let _: i64 = harness
        .redis
        .command("XGROUP", &key, &destroy)
        .await
        .expect("destroy the group");

    // The next read repairs it rather than failing, and finds nothing: the
    // historical entry is still in the stream, and must NOT be handed out.
    assert!(
        streams
            .read_new(&fleet, &consumer)
            .await
            .expect("the read must repair, not fail")
            .is_none(),
        "a repaired group must not re-deliver entries that already ran"
    );

    // And the repaired group works: an event appended after it is delivered.
    let fresh = streams
        .append(&fleet, &[("type", "after_repair")])
        .await
        .expect("append");
    let event = streams
        .read_new(&fleet, &consumer)
        .await
        .expect("read")
        .expect("the repaired group must deliver new events");
    assert_eq!(event.id, fresh);

    cleanup(&harness, &[key]).await;
}

/// The readiness index only clears a mark the caller actually saw.
///
/// The race this closes: a poll finds a fleet idle and moves to clear it while
/// ingress appends and re-marks. An unconditional delete erases a mark for
/// genuinely undelivered work, and nothing rediscovers it until a sweep.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_ready_index_clear_respects_the_token() {
    let harness = RedisHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());
    let fleet = harness.name("fleet");

    let observed = index.mark(&fleet, "token-a").await.expect("mark");
    assert!(
        index
            .peek(50)
            .await
            .expect("peek")
            .iter()
            .any(|ready| ready.fleet_id == fleet),
        "a marked fleet must be visible to a poll"
    );

    // Ingress marks again — a new generation — while the poll still holds the
    // token it read.
    index.mark(&fleet, "token-b").await.expect("re-mark");

    assert!(
        !index
            .clear_if_unchanged(&fleet, &observed)
            .await
            .expect("clear"),
        "a stale token must not clear a fleet that was re-marked"
    );
    assert!(
        index
            .peek(50)
            .await
            .expect("peek")
            .iter()
            .any(|ready| ready.fleet_id == fleet),
        "the newer mark must survive the stale clear"
    );

    // The current token does clear it.
    let current = index.mark(&fleet, "token-c").await.expect("mark");
    assert!(
        index
            .clear_if_unchanged(&fleet, &current)
            .await
            .expect("clear"),
        "the token the caller observed must clear the fleet"
    );

    cleanup_fields(&harness, &fleet).await;
}

/// Deletes the keys a test made. The lane resets Redis between runs; this keeps
/// one test's leftovers out of another's `peek` inside a run.
async fn cleanup(harness: &RedisHarness, keys: &[String]) {
    for key in keys {
        let mut cmd = redis::cmd("DEL");
        cmd.arg(key);
        let _: Result<i64, _> = harness.redis.command("DEL", key, &cmd).await;
    }
}

async fn cleanup_fields(harness: &RedisHarness, fleet: &str) {
    let mut cmd = redis::cmd("HDEL");
    cmd.arg(afd_redis::ready::READY_INDEX_KEY).arg(fleet);
    let _: Result<i64, _> = harness
        .redis
        .command("HDEL", afd_redis::ready::READY_INDEX_KEY, &cmd)
        .await;
}
