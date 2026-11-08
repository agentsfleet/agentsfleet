//! Which fleets a channel's mention can reach, read from their documents.
//!
//! Split from the route cases beside it: this drives the subscriber read on
//! its own, below the route, so what it asserts is the read's answer — every
//! fleet attached to the channel, with its flags — and not a routing verdict.

#![cfg(feature = "test-util")]

use afd_ingress::slack::ChannelId;

use super::*;

/// A channel no fixture fleet is attached to but one.
const OTHER_CHANNEL: &str = "C0987654321";

/// Dimension 2.2 — the subscriber read returns every fleet in the workspace
/// whose document attaches it to the channel, with whether it can run and
/// whether it is addressed-only, and nothing attached elsewhere.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn subscribers_are_read_from_the_document() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let responder = fixture
        .fleet(
            &document("responder", CHANNEL, Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let repairer = fixture
        .fleet(
            &document("repairer", CHANNEL, Some("write")),
            FleetStatus::Paused.as_str(),
        )
        .await;
    fixture
        .fleet(
            &document("elsewhere", OTHER_CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;

    let ingress = harness::unreachable_ingress(fixture.database());
    let channel: ChannelId = CHANNEL.parse().expect("a well-formed channel");
    let mut read = ingress
        .mention_subscribers(fixture.workspace(), PROVIDER.id(), &channel)
        .await
        .expect("the subscriber read runs");
    read.sort_by(|left, right| left.name.cmp(&right.name));
    let seen: Vec<(String, &str, bool, bool)> = read
        .iter()
        .map(|subscriber| {
            (
                subscriber.name.clone(),
                if subscriber.fleet == repairer {
                    "repairer"
                } else if subscriber.fleet == responder {
                    "responder"
                } else {
                    "other"
                },
                subscriber.runnable,
                subscriber.addressed_only,
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            ("repairer".to_owned(), "repairer", false, true),
            ("responder".to_owned(), "responder", true, false),
        ],
        "both attached fleets, and not the one attached elsewhere"
    );
    fixture.cleanup().await;
}
