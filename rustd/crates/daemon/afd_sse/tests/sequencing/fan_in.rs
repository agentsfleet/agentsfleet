//! Dimension 5.1, across fleets: a workspace fan-in over the bus attaches the
//! fleets it is told to, numbers frames across their channels, and skips what
//! it cannot route.
//!
//! Split from the single-channel ordering claims beside it by concern: those
//! are about one channel's order, this is about the fan-in's own attach set
//! and numbering.

use std::collections::BTreeSet;

use afd_bus::{Handle, Subject};
use afd_sse::FanIn;

use super::{DELIVERY_BUDGET, fleet, open_bus, payload};

/// The first of the workspace's two fleets; sorts before [`BETA`].
const ALPHA: &str = "0195b4ba-8d3a-7000-8abc-000000000002";

/// The second of the workspace's two fleets.
const BETA: &str = "0195b4ba-8d3a-7000-8abc-000000000003";

/// A workspace fan-in attaches only its authorised fleet set, numbers valid
/// frames across channels, drops an unrouteable payload without spending a
/// number, and detaches a fleet on the next authorization refresh.
#[tokio::test(flavor = "multi_thread")]
async fn test_workspace_fan_in_tracks_authorised_fleets_and_valid_frames() {
    let (bus, _server) = open_bus();
    let alpha = ALPHA.to_owned();
    let beta = BETA.to_owned();

    let mut fan_in = FanIn::new(Some(bus.clone()));
    let wanted = BTreeSet::from([beta.clone(), alpha.clone()]);
    let attached = fan_in.sync_to(&wanted).await;
    assert_eq!(attached.attached, 2);
    assert_eq!(attached.detached, 0);
    assert!(attached.is_change());
    assert_eq!(fan_in.fleets(), vec![alpha.clone(), beta.clone()]);
    assert!(!fan_in.sync_to(&wanted).await.is_change());
    assert_fan_in_delivery(&bus, &mut fan_in).await;

    let remaining = BTreeSet::from([beta.clone()]);
    let detached = fan_in.sync_to(&remaining).await;
    assert_eq!(detached.attached, 0);
    assert_eq!(detached.detached, 1);
    assert_eq!(fan_in.fleets(), vec![beta]);
}

async fn assert_fan_in_delivery(bus: &Handle, fan_in: &mut FanIn) {
    let beta_channel = Subject::FleetTail(fleet(BETA));
    bus.publish(&beta_channel, &payload(3))
        .await
        .expect("a valid fan-in frame reaches the bus");
    let first = tokio::time::timeout(DELIVERY_BUDGET, fan_in.next_frame())
        .await
        .expect("the fan-in yields its first frame");
    assert_eq!(first.seq, 0);
    assert!(
        first.data.text().contains(BETA),
        "the frame is tagged by its fleet"
    );

    let alpha_channel = Subject::FleetTail(fleet(ALPHA));
    bus.publish(&alpha_channel, "not-json")
        .await
        .expect("the malformed payload still reaches the channel");
    bus.publish(&alpha_channel, &payload(4))
        .await
        .expect("the valid payload follows it");
    let second = tokio::time::timeout(DELIVERY_BUDGET, fan_in.next_frame())
        .await
        .expect("the fan-in skips the malformed payload");
    assert_eq!(second.seq, 1);
    assert!(second.data.text().contains(ALPHA));
}
