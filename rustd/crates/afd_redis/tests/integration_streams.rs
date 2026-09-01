//! Dimension 3.1 — append, read as a group, acknowledge; plus the readiness
//! index's token rule, which is the part of it that is easy to get wrong.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_redis::Dedicated;
use afd_redis::streams::{FLEET_CONSUMER_GROUP, FleetStreams, fleet_stream_key};

use crate::support::RedisHarness;

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
    // The id renders as itself. `Display` is what puts an event id into a log
    // line and a `%`-sigil tracing field, and a `Display` that disagreed with
    // `as_str` would make the id in the logs unmatchable against the id in the
    // stream — the one thing an operator does with it.
    assert_eq!(
        appended.to_string(),
        appended.as_str(),
        "Display and as_str must be the same id"
    );
    assert!(
        !appended.to_string().is_empty(),
        "Redis mints a non-empty id"
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

/// Deletes the keys a test made. The lane resets Redis between runs; this keeps
/// one test's leftovers out of another's read inside a run.
async fn cleanup(harness: &RedisHarness, keys: &[String]) {
    for key in keys {
        let mut cmd = redis::cmd("DEL");
        cmd.arg(key);
        let _: Result<i64, _> = harness.redis.command("DEL", key, &cmd).await;
    }
}

/// A group create that fails for a reason OTHER than "it already exists" is
/// reported, not swallowed.
///
/// `ensure_group` treats `BUSYGROUP` as success, because a second caller
/// racing the first is the normal case. Every other failure has to travel: a
/// key already holding a non-stream value is a fleet id colliding with
/// something else in the keyspace, and silently continuing means every later
/// append to that fleet fails with no explanation of the first cause.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_a_group_create_that_is_not_a_race_is_reported() {
    let harness = RedisHarness::connect().await;
    let streams = FleetStreams::new(harness.redis.clone());
    let fleet = harness.name("wrongtype");
    let key = fleet_stream_key(&fleet);

    // Occupy the stream key with a string, so XGROUP answers WRONGTYPE.
    let mut set = redis::cmd("SET");
    set.arg(&key).arg("not-a-stream");
    let _: String = harness
        .redis
        .command("SET", &key, &set)
        .await
        .expect("seeding the key must work");

    let error = streams
        .ensure_group(&fleet)
        .await
        .expect_err("a WRONGTYPE key must not read as an idempotent create");
    let rendered = error.to_string();
    assert!(
        rendered.to_ascii_uppercase().contains("XGROUP"),
        "the failure must name the command that failed: {rendered}"
    );

    cleanup(&harness, &[key]).await;
}

/// A stale entry is claimed away from the consumer that abandoned it.
///
/// The crash-recovery path, and the only one that moves work between consumers.
/// An instance that read an entry and died still HOLDS it in the group's
/// pending list — no other consumer will ever be offered it by `XREADGROUP`,
/// so without this sweep that fleet's event is stranded until someone notices.
///
/// # The idle clock is forced, not waited out
///
/// `AUTOCLAIM_MIN_IDLE_MS` is five minutes, deliberately past the lease window
/// so the sweep cannot race live work. `XCLAIM … IDLE` sets the entry's idle
/// counter directly, which is how this asserts the real threshold rather than
/// lowering it for the test's convenience.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_an_abandoned_entry_is_autoclaimed_by_another_consumer() {
    let harness = RedisHarness::connect().await;
    let streams = FleetStreams::new(harness.redis.clone());
    let fleet = harness.name("fleet");
    let died = harness.name("consumer_that_died");
    let swept = harness.name("consumer_that_sweeps");

    streams.ensure_group(&fleet).await.expect("group create");
    let appended = streams
        .append(&fleet, &[("type", "message"), ("actor", "user_1")])
        .await
        .expect("append");

    // Delivered to the first consumer, which then never acknowledges it.
    let held = streams
        .read_new(&fleet, &died)
        .await
        .expect("read")
        .expect("the appended entry is delivered");
    assert_eq!(held.id, appended);

    // Nothing to claim yet: the entry is pending but freshly delivered, and
    // claiming it here would be the sweep racing a consumer still working.
    assert!(
        streams
            .autoclaim(&fleet, &swept)
            .await
            .expect("autoclaim")
            .is_none(),
        "a freshly-delivered entry must not be claimable"
    );

    let key = fleet_stream_key(&fleet);
    let mut claim = redis::cmd("XCLAIM");
    claim
        .arg(&key)
        .arg(FLEET_CONSUMER_GROUP)
        .arg(&died)
        .arg(0)
        .arg(appended.as_str())
        .arg("IDLE")
        .arg(400_000);
    let _: redis::Value = harness
        .redis
        .command("XCLAIM", &key, &claim)
        .await
        .expect("the entry's idle counter is forced past the threshold");

    let reclaimed = streams
        .autoclaim(&fleet, &swept)
        .await
        .expect("autoclaim")
        .expect("an entry idle past the threshold is claimable");

    assert_eq!(
        reclaimed.id, appended,
        "the claim moves the SAME entry rather than minting a second identity \
         for work already on the stream"
    );
    assert_eq!(
        reclaimed.field("actor"),
        Some("user_1"),
        "a claimed entry carries its fields, or the sweep hands a runner an \
         event it cannot act on"
    );
}

/// A dedicated connection reports the role it was opened for.
///
/// The role is what every failure on this socket names, and a background
/// consumer is the one holder that will not be watching when it fails. A
/// connection that forgot which role it serves would log an outage against the
/// wrong connection string.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_a_dedicated_connection_reports_the_role_it_opened_for() {
    let config = RedisHarness::config();
    let expected = config.role();

    let owned = Dedicated::connect(&config)
        .await
        .expect("the lane's Redis must be reachable");

    assert_eq!(
        owned.role(),
        expected,
        "the connection must carry the role it was opened for"
    );
}
