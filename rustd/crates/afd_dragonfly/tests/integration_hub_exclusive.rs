//! The hub recovers from a connection the SERVER tore down.
//!
//! `integration_hub` proves the recovery logic from inside: given that the
//! pump's connection ends, the hub redials and re-subscribes. This file proves
//! the half that logic cannot reach, and the half a datastore migration is
//! actually risking:
//!
//! ```text
//!   Dragonfly closes the socket
//!        -> TCP/TLS surfaces EOF or reset
//!             -> the redis-rs cluster driver pushes a Disconnection
//!                  -> the driver repairs the node and replays its SSUBSCRIBEs,
//!                     or the hub redials when no replay explains the loss
//!                       -> every reader of a lost channel is told: a gap
//! ```
//!
//! A test that ends the pump from inside this process starts at the fourth
//! arrow and asserts the fifth. Every arrow above it is the compatibility
//! surface being migrated, so skipping them turns a known gap into an untested
//! guarantee.
//!
//! # Why this file runs alone
//!
//! Dragonfly answers `CLIENT KILL TYPE pubsub` with a syntax error -- it
//! implements `ADDR`, `LADDR` and `ID` and nothing narrower -- and its
//! `CLIENT LIST` carries no `sub=`/`psub=` marker, defaulting a connection's
//! name to its own id. Measured against `dragonfly_version:df-v1.40.2`, which
//! reports `dragonfly_version:7.4.0` while implementing neither. So there is no
//! server-side handle for "the hub's connection", and the only way to find it
//! is to snapshot every node's clients, start the hub, and diff.
//!
//! That diff is only sound while nothing else opens a connection, which is why
//! this module is filtered out of the parallel lane and run again on its own
//! (`make/test-integration-rustd.mk`). It is a hard gate either way: the lane's
//! own guard fails a selection that matches nothing.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::Duration;

use afd_dragonfly::SubscriptionHub;
use afd_dragonfly::streams::{FleetStreams, fleet_activity_channel};
use backon::ExponentialBuilder;

use crate::cluster::{ClusterHarness, PRIMARY_A, PRIMARY_B};
use crate::hub_exclusive::{
    Node, clients_across, deliver, difference, gap_on, kill_each, nodes_of, owner_of,
    subscribers_on, wait_for,
};
use crate::support::DragonflyHarness;

/// Payload published before the kill, to prove the reader was live first.
const BEFORE: &str = "before-the-kill";

/// Payload published after it. Distinct from [`BEFORE`] so a stale frame
/// cannot be mistaken for recovery.
const AFTER: &str = "after-the-kill";

/// How many names are tried for a channel on each primary. Slots spread by
/// hash, so a handful of candidates lands on both halves of the canonical
/// split; this many failing to is a broken harness, not bad luck.
const CANDIDATES: u32 = 64;

/// A redial schedule a test can wait out.
fn impatient() -> ExponentialBuilder {
    ExponentialBuilder::new()
        .with_min_delay(Duration::from_millis(20))
        .with_max_delay(Duration::from_millis(100))
}

/// The hub survives the server killing its connection, and keeps delivering.
///
/// Every node's socket is killed, one `CLIENT KILL` at a time, so the hub may
/// see them as one loss it redials or as separate losses the driver repairs:
/// both are recoveries, and both end with the reader told about its gap.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_server_killed_connection_is_redialled_and_its_channels_resubscribed() {
    let harness = DragonflyHarness::connect().await;
    let publisher = FleetStreams::new(harness.redis.clone());
    let channel = harness.name("exclusive-channel");

    let nodes = nodes_of(&harness).await;
    assert!(
        !nodes.is_empty(),
        "the cluster advertises at least one node"
    );
    warm(&publisher, [&channel]).await;
    let (hub, before_hub) = start_hub(&nodes).await;
    let mut reader = hub.subscribe(&channel);
    deliver(&publisher, &channel, BEFORE, &mut reader).await;

    let opened = difference(&clients_across(&nodes).await, &before_hub);
    assert!(
        !opened.is_empty(),
        "the hub opened no connection this test can name, so the kill below \
         would prove nothing -- snapshots taken while another test was \
         connecting is the one way this goes wrong"
    );

    kill_each(&nodes, &opened).await;

    // The reader is told before anything else is asserted: a recovery that
    // re-subscribed silently would pass every count below.
    gap_on(&mut reader).await;
    // Not "went to zero first": a fast redial can make zero unobservable, and
    // a test that demanded it would fail on a healthy system that recovered
    // too quickly. One subscriber at the end is the claim.
    wait_for("the channel is re-subscribed", || async {
        subscribers_on(&harness, &channel).await == 1
    })
    .await;

    // The reader is the SAME one, held across the kill. Re-subscribing a
    // channel nobody is left listening to would satisfy every count above and
    // deliver nothing.
    deliver(&publisher, &channel, AFTER, &mut reader).await;
}

/// Dimension 5.2: one primary's socket lost is repaired inside the connection
/// the hub has. Its channels are re-subscribed and gapped, the other
/// primary's channel keeps delivering, and no connection is opened.
///
/// The other primary's channel may be gapped too: the hub cannot tell which
/// node was lost and re-subscribes every channel (see `hub::gap`), which is
/// an over-report a reader survives by backfilling.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_node_loss_is_a_gap_not_a_reconnect() {
    let harness = DragonflyHarness::connect().await;
    let publisher = FleetStreams::new(harness.redis.clone());
    let cluster = ClusterHarness::from_lane();
    let mut raw = cluster.connect().await;
    let (lost, kept) = channel_per_primary(&harness, &mut raw).await;
    let lost_slot = ClusterHarness::keyslot(&mut raw, &lost).await;

    let nodes = nodes_of(&harness).await;
    let owner = owner_of(&harness, lost_slot).await;
    warm(&publisher, [&lost, &kept]).await;
    let (hub, before_hub) = start_hub(&nodes).await;
    let mut lost_reader = hub.subscribe(&lost);
    let mut kept_reader = hub.subscribe(&kept);
    deliver(&publisher, &lost, BEFORE, &mut lost_reader).await;
    deliver(&publisher, &kept, BEFORE, &mut kept_reader).await;

    let opened = difference(&clients_across(&nodes).await, &before_hub);
    let victims: BTreeSet<_> = opened
        .into_iter()
        .filter(|(node, _)| *node == owner)
        .collect();
    assert!(
        !victims.is_empty(),
        "the hub holds no socket to {owner}, serving {lost}"
    );
    kill_each(&nodes, &victims).await;

    // The other primary's frames flow while the lost one is repaired.
    deliver(&publisher, &kept, "during-the-repair", &mut kept_reader).await;
    gap_on(&mut lost_reader).await;
    deliver(&publisher, &lost, AFTER, &mut lost_reader).await;
    deliver(&publisher, &kept, AFTER, &mut kept_reader).await;
    assert_eq!(
        hub.connections_opened(),
        1,
        "a node's repair happens inside the connection the hub has"
    );
}

/// The lane's client set, then a hub started on it. Every connection a test
/// owns is opened BEFORE this snapshot, so it appears in both and the hub's
/// own sockets are the difference.
async fn start_hub(nodes: &BTreeSet<Node>) -> (SubscriptionHub, BTreeSet<(Node, i64)>) {
    let before_hub = clients_across(nodes).await;
    let hub = SubscriptionHub::start_with_backoff(DragonflyHarness::config(), impatient())
        .await
        .expect("hub starts");
    (hub, before_hub)
}

/// Publishes once to each of `channels`, so the publisher has dialled every
/// node it will use before the client snapshot. Otherwise its own socket to
/// the lost node is in the difference, killed with the hub's, and its stalled
/// recovery reads as frames the hub failed to deliver.
async fn warm<const N: usize>(publisher: &FleetStreams, channels: [&String; N]) {
    for channel in channels {
        publisher
            .publish(channel, "warm")
            .await
            .expect("the publisher reaches every primary");
    }
}

/// One activity channel served by each canonical primary, lost first.
async fn channel_per_primary(
    harness: &DragonflyHarness,
    raw: &mut redis::cluster_async::ClusterConnection,
) -> (String, String) {
    let mut on_a = None;
    let mut on_b = None;
    for candidate in 0..CANDIDATES {
        let channel = fleet_activity_channel(&harness.name(&format!("node-{candidate}")));
        let slot = ClusterHarness::keyslot(raw, &channel).await;
        let side = if ClusterHarness::canonical_primary(slot) == PRIMARY_A {
            &mut on_a
        } else {
            &mut on_b
        };
        side.get_or_insert(channel);
        if let (Some(a), Some(b)) = (&on_a, &on_b) {
            return (a.clone(), b.clone());
        }
    }
    panic!("{CANDIDATES} names never reached both primaries {PRIMARY_A} and {PRIMARY_B}");
}
