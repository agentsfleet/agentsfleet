//! What the team's records say about the people in them, against live
//! Postgres: each change is logged once and names nobody, and an inviter with
//! no name is named in the email by the address they signed up with.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::test_util::trace::Capture;
use afd_tenant::team::Removal;

use crate::access_lane::{Fixture, id};

/// Now, for every call a case makes.
const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// The records one invite, its acceptance and a removal leave, by event.
const TEAM_EVENTS: [&str; 3] = [
    "workspace_invite_created",
    "workspace_invite_accepted",
    "workspace_member_removed",
];

/// What John's account is renamed to, so its name and his differ.
const ACCOUNT_NAME: &str = "john-account";

/// An invite, its acceptance and a removal are each logged once, and no record
/// carries an address or a person's name: ids are what an operator joins on.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_log_each_team_change_once_naming_nobody() {
    let fixture = Fixture::create().await;
    let capture = Capture::install();
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect("Carol accepts");
    let removed = fixture
        .team
        .remove(&id(&fixture.john.tenant), &fixture.carol.user_id)
        .await
        .expect("John removes Carol");
    assert_eq!(removed, Removal::Removed);

    let names = [fixture.john.name, fixture.carol.name];
    for event in TEAM_EVENTS {
        let record = capture.only(event);
        for value in record.fields.values() {
            assert!(!value.contains('@'), "{event} carries an address: {value}");
            assert!(
                names.iter().all(|name| !value.contains(name)),
                "{event} carries a name: {value}"
            );
        }
    }
    drop(capture);
    fixture.cleanup().await;
}

/// An inviter who never gave a name is named by their address, and the
/// account, whose owner has no name either, by its own name.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_name_a_nameless_inviter_by_address_and_the_account_by_its_name() {
    let fixture = Fixture::create().await;
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query("UPDATE core.users SET display_name = NULL WHERE id = $1::uuid")
        .bind(&fixture.john.user)
        .execute(&mut *connection)
        .await
        .expect("John's name clears");
    sqlx::query("UPDATE core.tenants SET name = $2 WHERE id = $1::uuid")
        .bind(&fixture.john.tenant)
        .bind(ACCOUNT_NAME)
        .execute(&mut *connection)
        .await
        .expect("John's account renames");
    drop(connection);

    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let attempt = fixture
        .begin_email(&invite, NOW)
        .await
        .expect("the invite is still pending");
    assert_eq!(attempt.inviter_name, fixture.john.email);
    assert_eq!(attempt.owner_name, ACCOUNT_NAME);
    fixture.cleanup().await;
}
