//! Dimension 4.6 for invites: `resolved_at` is the one pending marker. Every
//! resolving statement is guarded on it, and the one-pending-invite index reads
//! it, so an accepted invite is never resolved again and a second pending
//! invite for one address is refused by Postgres itself, not only by the
//! service's own check.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_tenant::error::InviteConflict;
use afd_tenant::sql::invite::{INSERT_INVITE, MARK_ACCEPTED, REVOKE_INVITE};
use afd_tenant::team::Email;
use jiff::Timestamp;

use crate::access_lane::{Fixture, id};

/// The instant each identifier a case mints is minted at.
const NOW: Timestamp = Timestamp::constant(1_767_225_600, 0);

/// The index that admits one pending invite per address per tenant.
const PENDING_INDEX: &str = "uq_invites_tenant_id_email_pending";

/// How long a hand-inserted invite stays open.
const OPEN_FOR: Duration = Duration::from_secs(3_600);

/// `(status, resolved_at, accepted_by_user_id)` as stored, text for exactness.
async fn resolution(fixture: &Fixture, invite: &Uuid7) -> (String, Option<String>, Option<String>) {
    let mut connection = fixture.database.acquire().await.expect("a connection");
    sqlx::query_as(
        "SELECT status, resolved_at::text, accepted_by_user_id::text
           FROM identity.invites WHERE id = $1::uuid",
    )
    .bind(invite.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("the invite reads")
}

/// The issuing statement run by hand, past the service's own conflict check.
async fn insert_pending(fixture: &Fixture, address: &str) -> Result<String, sqlx::Error> {
    let email = Email::parse(address, |_| true).expect("a fixture address parses");
    let invite = mint_id();
    let mut connection = fixture.database.acquire().await.expect("a connection");
    sqlx::query(INSERT_INVITE)
        .bind(&invite)
        .bind(&fixture.john.tenant)
        .bind(email.as_str())
        .bind("member")
        .bind("pending")
        .bind(&fixture.john.user)
        .bind(OPEN_FOR)
        .execute(&mut *connection)
        .await
        .map(|_| invite)
}

/// An accepted invite keeps its first resolution: another accept is refused,
/// and the guarded accept and revoke statements touch no row.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_resolution_happens_once_for_an_accepted_invite() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect("Carol joins");
    let first = resolution(&fixture, &invite).await;
    assert_eq!(first.0, "accepted");
    assert!(first.1.is_some(), "an accept stamps resolved_at");
    assert_eq!(first.2.as_deref(), Some(fixture.carol.user.as_str()));

    let again = fixture
        .team
        .accept(&invite, &fixture.bob.invitee(), NOW)
        .await
        .expect_err("an accepted invite is not accepted again");
    assert_eq!(again.code(), error_code::INVITE_NOT_FOUND);

    let mut connection = fixture.database.acquire().await.expect("a connection");
    let accepted = sqlx::query(MARK_ACCEPTED)
        .bind(invite.as_str())
        .bind(&fixture.bob.user)
        .bind("accepted")
        .execute(&mut *connection)
        .await
        .expect("the guarded accept runs");
    assert_eq!(
        accepted.rows_affected(),
        0,
        "the accept is guarded on resolved_at"
    );
    let revoked = sqlx::query(REVOKE_INVITE)
        .bind(&fixture.john.tenant)
        .bind(invite.as_str())
        .bind("revoked")
        .execute(&mut *connection)
        .await
        .expect("the guarded revoke runs");
    assert_eq!(
        revoked.rows_affected(),
        0,
        "the revoke is guarded on resolved_at"
    );
    drop(connection);

    assert_eq!(
        resolution(&fixture, &invite).await,
        first,
        "neither statement re-stamped the first resolution"
    );
    fixture.cleanup().await;
}

/// A second pending invite for one address is refused by the partial unique
/// index; once the first resolves, the index admits a new one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_second_pending_invite_for_one_address_is_refused_by_the_index() {
    let fixture = Fixture::create().await;
    let first = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");

    let refused = insert_pending(&fixture, &fixture.carol.email)
        .await
        .expect_err("one pending invite per address");
    let constraint = refused
        .as_database_error()
        .and_then(|database| database.constraint())
        .map(str::to_owned);
    assert_eq!(constraint.as_deref(), Some(PENDING_INDEX));

    fixture
        .team
        .revoke_invitation(&id(&fixture.john.tenant), &first)
        .await
        .expect("the revoke lands");
    let second = insert_pending(&fixture, &fixture.carol.email)
        .await
        .expect("a resolved invite no longer holds the index");

    let third = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect_err("the hand-issued invite is pending now");
    assert_eq!(third.code(), error_code::INVITE_CONFLICT);
    assert_eq!(third.invite_conflict(), Some(InviteConflict::Invited));
    let pending = resolution(&fixture, &id(&second)).await;
    assert_eq!(pending, ("pending".to_owned(), None, None));
    fixture.cleanup().await;
}
