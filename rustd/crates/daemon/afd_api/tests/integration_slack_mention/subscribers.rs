//! Which fleets a channel's mention can reach, read from their `mention`
//! rows, and the route that delivers to them.
//!
//! The first case drives the subscriber read on its own, below the route, so
//! what it asserts is the read's answer — every fleet subscribed to the
//! channel, with its flags — and not a routing verdict.

#![cfg(feature = "test-util")]

use af_fleet_runtime::config::ChannelId;

use super::*;

/// A channel no fixture fleet is attached to but one.
const OTHER_CHANNEL: &str = "C0987654321";

/// The subscriber read returns every fleet in the workspace whose `mention`
/// row names the channel, with whether it can run and whether its approved
/// GitHub reach makes it addressed-only, and nothing subscribed elsewhere.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn subscribers_are_read_from_their_rows() {
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
            FleetStatus::Archived.as_str(),
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

/// A mention reaches the one fleet whose row names its channel, not one
/// subscribed elsewhere; with two fleets on the channel, the named one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_mention_reaches_the_fleet_subscribed_to_its_channel() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let responder = fixture
        .fleet(
            &document("responder", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    fixture
        .fleet(
            &document("elsewhere", OTHER_CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();

    let alone = mention(
        &fixture.team,
        "EvRowed01",
        PERSON,
        &format!("<@{BOT_USER}> is checkout up?"),
    );
    assert_eq!(deliver(&router, &alone).await.status(), StatusCode::OK);
    let admitted = fixture
        .turn(&key(&fixture.team, "EvRowed01"))
        .await
        .expect("the mention was admitted");
    assert_eq!(
        admitted.fleet, responder,
        "the channel's only fleet takes it"
    );

    let triage = fixture
        .fleet(
            &document("triage", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let named = mention(
        &fixture.team,
        "EvRowed02",
        PERSON,
        &format!("<@{BOT_USER}> Triage: why did it fail?"),
    );
    assert_eq!(deliver(&router, &named).await.status(), StatusCode::OK);
    let admitted = fixture
        .turn(&key(&fixture.team, "EvRowed02"))
        .await
        .expect("the named mention was admitted");
    assert_eq!(admitted.fleet, triage, "two on one channel: the name picks");
    assert_eq!(fixture.turns().await, 2, "one turn per mention, no more");
    fixture.cleanup().await;
}
