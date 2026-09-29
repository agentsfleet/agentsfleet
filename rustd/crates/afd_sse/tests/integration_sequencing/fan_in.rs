//! Dimension 5.1, across fleets: a workspace fan-in over a live hub attaches
//! the fleets it is told to, numbers frames across their channels, and skips
//! what it cannot route.
//!
//! Split from the single-channel ordering claims beside it by concern: those
//! are about one channel's transport order, this is about the fan-in's own
//! attach set and numbering.

use std::collections::BTreeSet;

use afd_dragonfly::SubscriptionHub;
use afd_dragonfly::streams::FleetStreams;
use afd_sse::FanIn;
use afd_sse::channel;

use super::support::{DELIVERY_BUDGET, SseLane};
use super::{payload, prime};

/// A workspace fan-in attaches only its authorised fleet set, numbers valid
/// frames across channels, drops an unrouteable payload without spending a
/// number, and detaches a fleet on the next authorization refresh.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_workspace_fan_in_tracks_authorised_fleets_and_valid_frames() {
    let lane = SseLane::connect().await;
    let publisher = FleetStreams::new(lane.redis.clone());
    let alpha = lane.fleet("fanin-alpha");
    let beta = lane.fleet("fanin-beta");
    let alpha_channel = channel::activity(&alpha);
    let beta_channel = channel::activity(&beta);
    let hub = SubscriptionHub::start(SseLane::config())
        .await
        .expect("the hub starts");

    // Prime each server-side subscription before the fan-in joins its local
    // broadcast. Once the channel exists, `subscribe` adds a receiver without
    // a second Dragonfly round trip, so no fixed sleep is involved.
    let mut alpha_primer = hub.subscribe(&alpha_channel);
    prime(&publisher, &alpha_channel, &mut alpha_primer).await;
    let mut beta_primer = hub.subscribe(&beta_channel);
    prime(&publisher, &beta_channel, &mut beta_primer).await;

    let mut fan_in = FanIn::new(Some(hub));
    let wanted = BTreeSet::from([beta.clone(), alpha.clone()]);
    let attached = fan_in.sync_to(&wanted);
    assert_eq!(attached.attached, 2);
    assert_eq!(attached.detached, 0);
    assert!(attached.is_change());
    assert_eq!(fan_in.fleets(), vec![alpha.clone(), beta.clone()]);
    assert!(!fan_in.sync_to(&wanted).is_change());
    assert_fan_in_delivery(&publisher, &mut fan_in, &alpha, &beta).await;

    let remaining = BTreeSet::from([beta.clone()]);
    let detached = fan_in.sync_to(&remaining);
    assert_eq!(detached.attached, 0);
    assert_eq!(detached.detached, 1);
    assert_eq!(fan_in.fleets(), vec![beta]);
}

async fn assert_fan_in_delivery(
    publisher: &FleetStreams,
    fan_in: &mut FanIn,
    alpha: &str,
    beta: &str,
) {
    let beta_channel = channel::activity(beta);
    publisher
        .publish(&beta_channel, &payload(3))
        .await
        .expect("a valid fan-in frame reaches Dragonfly");
    let first = tokio::time::timeout(DELIVERY_BUDGET, fan_in.next_frame())
        .await
        .expect("the fan-in yields its first frame");
    assert_eq!(first.seq, 0);
    assert!(
        first.data.text().contains(beta),
        "the frame is tagged by its fleet"
    );

    let alpha_channel = channel::activity(alpha);
    publisher
        .publish(&alpha_channel, "not-json")
        .await
        .expect("the malformed payload still reaches the channel");
    publisher
        .publish(&alpha_channel, &payload(4))
        .await
        .expect("the valid payload follows it");
    let second = tokio::time::timeout(DELIVERY_BUDGET, fan_in.next_frame())
        .await
        .expect("the fan-in skips the malformed payload");
    assert_eq!(second.seq, 1);
    assert!(second.data.text().contains(alpha));
}
