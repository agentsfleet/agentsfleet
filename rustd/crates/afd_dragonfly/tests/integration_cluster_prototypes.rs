//! What the driver and the cluster do under slot movement, learned before the
//! boundary is built over them.
//!
//! Two proofs, both against the lane's real four-node cluster and both
//! holding the cluster lane, because each moves a slot: a sharded
//! subscription's fate when its channel's slot migrates, and whether a
//! single-key script and a parked blocking read keep answering while their
//! key moves. The two-key script the crate ships today is pinned here as the
//! `CROSSSLOT` it is, so the next reader does not have to rediscover it.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use redis::cluster_async::ClusterConnection;
use redis::cluster_routing::{Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};
use redis::{PushInfo, PushKind, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::cluster::{CLUSTER_LANE, ClusterHarness, push_text};

/// How long a push may take to travel publisher → node → driver → receiver.
const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// How long the parked reader is given across a migration, generously: the
/// server's park, a redirect, a slot refresh and a retry all fit inside it.
const PARKED_READ_BUDGET: Duration = Duration::from_secs(20);

/// The longest single park the reader requests from the server, in the
/// milliseconds `XREADGROUP BLOCK` itself speaks. The command's unit is the
/// source of truth here so the argument needs no conversion.
const PARK_MILLIS: u64 = 3_000;

/// The same park as a `Duration`, for the connection's deadline arithmetic.
const PARK: Duration = Duration::from_millis(PARK_MILLIS);

/// How long a publish waits before it is sure a stranded subscription is
/// stranded rather than slow.
const STRANDED_BUDGET: Duration = Duration::from_secs(2);

const CMD_SPUBLISH: &str = "SPUBLISH";
const CMD_XGROUP: &str = "XGROUP";
const CMD_XADD: &str = "XADD";
const CMD_XREADGROUP: &str = "XREADGROUP";
const CMD_EVAL: &str = "EVAL";

/// A counter that sets its window on first touch — the shape `kv.rs` ships,
/// single-key by construction.
const INCREMENT_IN_WINDOW: &str = r"
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
return count
";

/// Two keys in one script: the shape `streams/once.rs` ships today.
const TWO_KEY_SCRIPT: &str = r"
redis.call('SET', KEYS[1], ARGV[1])
return redis.call('GET', KEYS[2])
";

/// The reply code the server gives a multi-key operation across slots.
const CROSSSLOT: &str = "CROSSSLOT";

/// Dimension 0.1 — a sharded subscription made through one seed receives a
/// `SPUBLISH` issued through another, and what happens to it when the
/// channel's slot moves.
///
/// The second half is the open question the probe left: the server answered a
/// misplaced `SSUBSCRIBE` with `OK` rather than `MOVED`, so nothing guarantees
/// a subscriber is told its channel has gone elsewhere. This test accepts
/// either outcome and RECORDS which one happened — delivery carried across
/// the move, or the subscription stranded and a re-issued `SSUBSCRIBE`
/// restoring it — because both are designs a hub can be built on, and only
/// the second needs the hub to do anything. What it refuses is a duplicate
/// frame in either case.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Dragonfly cluster: make test-integration-rustd"]
async fn test_sharded_subscription_survives_slot_movement() {
    let _lane = CLUSTER_LANE.lock().await;
    let harness = ClusterHarness::from_lane();
    let channel = harness.name("activity");
    let (mut subscriber, mut pushes) = harness.connect_with_pushes().await;
    let mut publisher = harness.connect().await;

    let slot = ClusterHarness::keyslot(&mut publisher, &channel).await;
    let owner = ClusterHarness::canonical_primary(slot);
    let target = ClusterHarness::other_primary(owner);

    subscriber.ssubscribe(&channel).await.expect("ssubscribe");
    await_push(&mut pushes, PushKind::SSubscribe, DELIVERY_BUDGET).await;

    // Cross-seed delivery: a distinct client, routed by the channel's slot.
    assert_eq!(spublish(&mut publisher, &channel, "one").await, 1);
    let first = await_message(&mut pushes, DELIVERY_BUDGET).await;
    assert_eq!(first.as_deref(), Some("one"));

    harness.move_slot(slot, owner, target).await;

    let receivers_after_move = spublish(&mut publisher, &channel, "two").await;
    let carried = matches!(
        await_message(&mut pushes, STRANDED_BUDGET).await.as_deref(),
        Some("two")
    );
    if carried {
        println!("evidence: the driver carried the sharded subscription across slot movement");
    } else {
        // Stranded: the new owner counts no subscriber, and the reader hears
        // nothing. Re-issuing the subscription is the recovery a hub performs.
        assert_eq!(
            receivers_after_move, 0,
            "a subscription the new owner does not know about must not be counted"
        );
        subscriber.ssubscribe(&channel).await.expect("resubscribe");
        await_push(&mut pushes, PushKind::SSubscribe, DELIVERY_BUDGET).await;
        assert_eq!(spublish(&mut publisher, &channel, "three").await, 1);
        assert_eq!(
            await_message(&mut pushes, DELIVERY_BUDGET).await.as_deref(),
            Some("three"),
            "a re-issued SSUBSCRIBE restores delivery from the new owner"
        );
        println!(
            "evidence: slot movement stranded the sharded subscription; re-issuing SSUBSCRIBE restored it"
        );
    }

    // No frame arrives twice, whichever path was taken.
    let duplicates = drain_messages(&mut pushes, STRANDED_BUDGET).await;
    assert!(
        duplicates.is_empty(),
        "no message may be delivered a second time after the move: {duplicates:?}"
    );

    harness.move_slot(slot, target, owner).await;
}

/// Dimension 0.2 — a single-key script and a parked blocking read keep
/// answering while their key's slot moves, and the two-key script the crate
/// ships is refused with `CROSSSLOT`.
///
/// The stream shares the counter's hash tag, so one migration moves both and
/// the co-location the target layout relies on is exercised in passing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Dragonfly cluster: make test-integration-rustd"]
async fn test_cluster_primitives_survive_movement() {
    let _lane = CLUSTER_LANE.lock().await;
    let harness = ClusterHarness::from_lane();
    let mut connection = harness.connect().await;
    let counter = harness.name("counter");
    let stream = format!("{{{counter}}}:stream");
    let group = harness.name("group");
    let consumer = harness.name("consumer");

    let slot = ClusterHarness::keyslot(&mut connection, &counter).await;
    assert_eq!(
        ClusterHarness::keyslot(&mut connection, &stream).await,
        slot,
        "a shared hash tag puts two keys in one slot"
    );
    let owner = ClusterHarness::canonical_primary(slot);
    let target = ClusterHarness::other_primary(owner);

    let script = redis::Script::new(INCREMENT_IN_WINDOW);
    let first: i64 = script
        .key(&counter)
        .arg(600)
        .invoke_async(&mut connection)
        .await
        .expect("a single-key script runs where its key lives");
    assert_eq!(first, 1);

    let mut xgroup = redis::cmd(CMD_XGROUP);
    xgroup
        .arg("CREATE")
        .arg(&stream)
        .arg(&group)
        .arg("$")
        .arg("MKSTREAM");
    let _created: String = xgroup
        .query_async(&mut connection)
        .await
        .expect("group create");

    // Park a dedicated reader, move the slot under it, then append.
    let mut parked = harness.connect_parked(PARK).await;
    let read_stream = stream.clone();
    let reader =
        tokio::spawn(
            async move { parked_read(&mut parked, &read_stream, &group, &consumer).await },
        );
    tokio::time::sleep(Duration::from_millis(300)).await;

    harness.move_slot(slot, owner, target).await;

    let mut xadd = redis::cmd(CMD_XADD);
    xadd.arg(&stream).arg("*").arg("kind").arg("probe");
    let appended: String = xadd
        .query_async(&mut connection)
        .await
        .expect("an append follows the slot to its new owner");

    let delivered = tokio::time::timeout(PARKED_READ_BUDGET, reader)
        .await
        .expect("the parked reader answers inside its budget")
        .expect("the reader task is not cancelled");
    assert!(
        delivered.contains(&appended),
        "the entry appended after the move reaches the parked reader: {delivered}"
    );

    let second: i64 = script
        .key(&counter)
        .arg(600)
        .invoke_async(&mut connection)
        .await
        .expect("the script follows its key and reloads on the new owner");
    assert_eq!(second, 2, "the moved counter kept its value");

    // The crate's two-key shape, routed to the owner explicitly so the reply
    // is the SERVER's verdict rather than the driver's.
    let other = key_in_another_slot(&mut connection, &harness, slot).await;
    let mut eval = redis::cmd(CMD_EVAL);
    eval.arg(TWO_KEY_SCRIPT)
        .arg(2)
        .arg(&counter)
        .arg(&other)
        .arg("1");
    // Under RESP3 a routed command hands back the server's error as a VALUE,
    // not an `Err`; `extract_error` is what turns it into one.
    let refused = connection
        .route_command(
            eval,
            RoutingInfo::SingleNode(SingleNodeRoutingInfo::SpecificNode(Route::new(
                slot,
                SlotAddr::Master,
            ))),
        )
        .await
        .expect("the routed command reaches the owner")
        .extract_error()
        .expect_err("two keys in different slots cannot share a script");
    assert_eq!(refused.code(), Some(CROSSSLOT), "{refused}");

    harness.move_slot(slot, target, owner).await;
}

/// One blocking group read, retried across the redirect a migration answers
/// with, returning the raw reply's rendering once an entry arrives.
async fn parked_read(
    connection: &mut ClusterConnection,
    stream: &str,
    group: &str,
    consumer: &str,
) -> String {
    let deadline = tokio::time::Instant::now() + PARKED_READ_BUDGET;
    loop {
        let mut read = redis::cmd(CMD_XREADGROUP);
        read.arg("GROUP")
            .arg(group)
            .arg(consumer)
            .arg("BLOCK")
            .arg(PARK_MILLIS)
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(stream)
            .arg(">");
        match read.query_async::<Value>(connection).await {
            Ok(Value::Nil) => {}
            Ok(entries) => return format!("{entries:?}"),
            Err(failure) => println!("evidence: parked read retried after: {failure}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the parked reader never saw the entry"
        );
    }
}

/// A key whose slot is not `slot`, found by trying suffixes.
async fn key_in_another_slot(
    connection: &mut ClusterConnection,
    harness: &ClusterHarness,
    slot: u16,
) -> String {
    for attempt in 0..64_u8 {
        let candidate = harness.name(&format!("other{attempt}"));
        if ClusterHarness::keyslot(connection, &candidate).await != slot {
            return candidate;
        }
    }
    panic!("sixty-four names all hashed to one slot, which is not a hash function");
}

async fn spublish(connection: &mut ClusterConnection, channel: &str, payload: &str) -> i64 {
    let mut cmd = redis::cmd(CMD_SPUBLISH);
    cmd.arg(channel).arg(payload);
    cmd.query_async(connection)
        .await
        .expect("SPUBLISH routes by the channel's slot")
}

/// Waits for a push of `kind`, skipping others.
async fn await_push(pushes: &mut UnboundedReceiver<PushInfo>, kind: PushKind, budget: Duration) {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        let push = tokio::time::timeout_at(deadline, pushes.recv())
            .await
            .unwrap_or_else(|_elapsed| panic!("no {kind:?} push within {budget:?}"))
            .expect("the push channel outlives the test");
        if push.kind == kind {
            return;
        }
    }
}

/// The next message payload, or `None` when none arrives inside `budget`.
async fn await_message(
    pushes: &mut UnboundedReceiver<PushInfo>,
    budget: Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        let push = tokio::time::timeout_at(deadline, pushes.recv())
            .await
            .ok()??;
        if push.kind == PushKind::SMessage {
            return push_text(&push, 1);
        }
        println!(
            "evidence: push while waiting for a message: {:?}",
            push.kind
        );
    }
}

/// Every message payload that arrives inside `budget`.
async fn drain_messages(pushes: &mut UnboundedReceiver<PushInfo>, budget: Duration) -> Vec<String> {
    let mut seen = Vec::new();
    while let Some(payload) = await_message(pushes, budget).await {
        seen.push(payload);
    }
    seen
}
