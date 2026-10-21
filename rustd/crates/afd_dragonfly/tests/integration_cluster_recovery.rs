//! Dimensions 5.2 and 5.3 — what survives a slot moving underneath the
//! surfaces this crate exposes.
//!
//! The §0 prototypes proved the primitives hold across a migration, driving
//! the raw driver. These drive the BOUNDARY: `FleetStreams`, `Dragonfly::scan_keys`
//! and `SubscriptionHub`, which is what the daemon actually calls, and assert
//! the properties a caller of those depends on.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use afd_dragonfly::hub::{Received, Subscription};
use afd_dragonfly::streams::{FleetStreams, fleet_activity_channel};
use afd_dragonfly::{Dedicated, SubscriptionHub};

use crate::cluster::{CLUSTER_LANE, ClusterHarness};
use crate::support::DragonflyHarness;

/// How long a frame or a reply is waited for before the test fails.
const BUDGET: Duration = Duration::from_secs(20);

/// How many keys the scan is asked to find, spread over slots by their names.
const SCANNED_KEYS: u32 = 24;

/// How long a scanned key lives, comfortably past the test.
const KEY_TTL_SECONDS: i64 = 300;

/// How long the dedicated reader parks, and the ceiling a command on the
/// SHARED handle must answer well inside of while it does.
const PARK: Duration = Duration::from_secs(5);
const SHARED_REPLY_CEILING: Duration = Duration::from_secs(1);

/// A node's slots moving restores every subscription and leaves every scan
/// finding all of its keys, and a parked blocking read never shares a socket.
///
/// Three claims, graded together because they are one question: after the
/// topology changes underneath it, does a caller of this boundary still see
/// everything it saw before.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Dragonfly cluster: make test-integration-rustd"]
async fn test_cluster_connections_recover_without_missing_scoped_state() {
    let _lane = CLUSTER_LANE.lock().await;
    let cluster = ClusterHarness::from_lane();
    let harness = DragonflyHarness::connect().await;
    let mut raw = cluster.connect().await;

    // ── the subscription ────────────────────────────────────────────────
    let fleet = harness.name("recovered");
    let channel = fleet_activity_channel(&fleet);
    let hub = SubscriptionHub::start(DragonflyHarness::config())
        .await
        .expect("the hub opens on the lane's cluster");
    let mut reader = hub.subscribe(&channel);
    let streams = FleetStreams::new(harness.redis.clone());
    // Before the move, so the failure of the assertion after it is the move
    // and not a subscription that never worked.
    deliver(&streams, &fleet, "before", &mut reader).await;

    let channel_slot = ClusterHarness::keyslot(&mut raw, &channel).await;
    let channel_owner = ClusterHarness::canonical_primary(channel_slot);
    cluster
        .move_slot(
            channel_slot,
            channel_owner,
            ClusterHarness::other_primary(channel_owner),
        )
        .await;
    deliver(&streams, &fleet, "after", &mut reader).await;
    assert_eq!(
        hub.connections_opened(),
        1,
        "a slot moving must not cost a second connection: the hub re-subscribes on the one it has"
    );

    // ── the scan ────────────────────────────────────────────────────────
    let prefix = harness.name("scanned");
    let mut written = BTreeSet::new();
    for n in 0..SCANNED_KEYS {
        let key = format!("{prefix}:{n}");
        harness
            .redis
            .set_for(&key, "v", KEY_TTL_SECONDS)
            .await
            .expect("the key is written");
        written.insert(key);
    }
    let glob = format!("{prefix}:*");
    let before: BTreeSet<String> = harness
        .redis
        .scan_keys(&glob, 100)
        .await
        .expect("the scan answers")
        .into_iter()
        .collect();
    assert_eq!(
        before, written,
        "a scan finds every key across the primaries"
    );

    // The keys are spread over slots by their names, so moving one range
    // relocates some of them and leaves the rest where they were.
    let moved_slot = ClusterHarness::keyslot(&mut raw, &format!("{prefix}:0")).await;
    let moved_owner = ClusterHarness::canonical_primary(moved_slot);
    cluster
        .move_slot(
            moved_slot,
            moved_owner,
            ClusterHarness::other_primary(moved_owner),
        )
        .await;
    let after: BTreeSet<String> = harness
        .redis
        .scan_keys(&glob, 100)
        .await
        .expect("the scan answers after the move")
        .into_iter()
        .collect();
    assert_eq!(
        after, written,
        "a scan still finds every key once a slot has moved: it walks the live topology"
    );

    a_parked_read_never_stalls_the_shared_connection(&harness).await;

    // ── cleanup ─────────────────────────────────────────────────────────
    for key in &written {
        let mut del = redis::cmd("DEL");
        del.arg(key);
        let _removed: Result<i64, _> = harness.redis.command("DEL", key, &del).await;
    }
    cluster
        .move_slot(
            moved_slot,
            ClusterHarness::other_primary(moved_owner),
            moved_owner,
        )
        .await;
    cluster
        .move_slot(
            channel_slot,
            ClusterHarness::other_primary(channel_owner),
            channel_owner,
        )
        .await;
    drop(reader);
    hub.shutdown();
}

/// A parked blocking read holds ITS connection, never the shared one.
///
/// Graded by what a caller would notice: a command on the shared handle
/// answers at once while the dedicated one is parked for its whole interval.
async fn a_parked_read_never_stalls_the_shared_connection(harness: &DragonflyHarness) {
    // A parked read holds ITS connection, never the shared one. Graded by
    // what a caller would notice: a command on the shared handle answers at
    // once while the dedicated one is parked for its whole interval.
    let mut parked = Dedicated::connect(&DragonflyHarness::config(), PARK)
        .await
        .expect("the dedicated reader opens its own connection");
    let blocking = tokio::spawn(async move {
        let mut cmd = redis::cmd("BLPOP");
        cmd.arg("cluster-recovery-nothing-lands-here").arg(1);
        let _timed_out: Result<Option<Vec<String>>, _> = parked
            .command("BLPOP", "cluster-recovery-nothing-lands-here", &cmd)
            .await;
    });
    let started = Instant::now();
    harness
        .redis
        .ping()
        .await
        .expect("the shared connection answers while the dedicated one is parked");
    let waited = started.elapsed();
    assert!(
        waited < SHARED_REPLY_CEILING,
        "a parked read stalled the shared connection for {waited:?}: it is not on its own socket"
    );
    blocking
        .await
        .expect("the parked read ends on its own deadline");
}
/// Publishes `payload` on the fleet's tail and asserts the reader sees it.
///
/// Retried rather than published once: a subscription re-issued after a slot
/// moves is re-issued when the server's push ARRIVES, and a publish that
/// raced that push would be a frame nobody was subscribed for yet.
async fn deliver(streams: &FleetStreams, fleet: &str, payload: &str, reader: &mut Subscription) {
    let seen = tokio::time::timeout(BUDGET, async {
        loop {
            streams
                .publish_tail(fleet, payload)
                .await
                .expect("the publish reaches the owning node");
            if let Ok(Ok(Received::Message(message))) =
                tokio::time::timeout(Duration::from_millis(200), reader.recv()).await
                && message.payload == payload
            {
                return;
            }
        }
    })
    .await;
    assert!(seen.is_ok(), "the reader never saw {payload}");
}
