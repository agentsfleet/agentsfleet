//! An archived fleet is never a channel's notice carrier: the carrier is the
//! channel's oldest runnable fleet, and a channel with none drops the notice
//! with one named reason. No test here binds a carrier first.
#![cfg(feature = "test-util")]

use super::notice::owed;
use super::resident_bound::{bound, owner_document};
use super::*;

/// The reason a notice no active fleet can carry is dropped under.
const REASON_NO_ACTIVE_CARRIER: &str = "no_active_notice_carrier";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_archived_fleets_notice_is_carried_by_an_active_one() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    // Seeded first, so its id is the oldest: an id-ordered pick would take it.
    fixture
        .fleet(&owner_document(), FleetStatus::Archived.as_str())
        .await;
    let active = fixture
        .fleet(
            &document("responder", CHANNEL, Some("write")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let body = mention(
        &fixture.team,
        "EvArchivedCarrier01",
        PERSON,
        &format!("<@{BOT_USER}> notice-carrier: who is on call?"),
    );
    assert_eq!(
        deliver(&fixture.router(), &body).await.status(),
        StatusCode::OK
    );
    assert_eq!(fixture.admissions().await, 0);
    let notices = owed(&fixture).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].owner, "responder");
    assert!(notices[0].answer.contains("notice-carrier is archived"));
    assert_eq!(bound(&fixture).await.0, active, "the active fleet is bound");
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_unaddressed_notice_never_binds_an_archived_fleet() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(&owner_document(), FleetStatus::Archived.as_str())
        .await;
    let addressed_only = fixture
        .fleet(
            &document("repairer", CHANNEL, Some("write")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let body = mention(
        &fixture.team,
        "EvArchivedCarrier02",
        PERSON,
        &format!("<@{BOT_USER}> who is on call?"),
    );
    assert_eq!(
        deliver(&fixture.router(), &body).await.status(),
        StatusCode::OK
    );
    let notices = owed(&fixture).await;
    assert_eq!(notices.len(), 1, "the address-it notice is owed");
    assert_eq!(notices[0].owner, "repairer");
    assert_eq!(bound(&fixture).await.0, addressed_only);
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_channel_with_only_archived_fleets_drops_the_notice_by_name() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(&owner_document(), FleetStatus::Archived.as_str())
        .await;
    let body = mention(
        &fixture.team,
        "EvArchivedCarrier03",
        PERSON,
        &format!("<@{BOT_USER}> notice-carrier: who is on call?"),
    );
    let answer = json_body(deliver(&fixture.router(), &body).await).await;
    assert_eq!(
        answer.get("ignored").and_then(Value::as_str),
        Some(REASON_NO_ACTIVE_CARRIER)
    );
    assert!(owed(&fixture).await.is_empty());
    assert_eq!(fixture.admissions().await, 0);
    fixture.cleanup().await;
}
