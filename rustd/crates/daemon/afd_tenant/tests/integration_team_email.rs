//! The invite email's store half against live Postgres: an attempt is counted
//! before its send and names what the email says, a result is recorded only
//! while its attempt is the latest, and a send that never recorded reads as
//! failed.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_tenant::team::EmailStatus;
use jiff::Timestamp;

use crate::access_lane::{Fixture, id};

/// The instant each identifier a case mints is minted at. Every stamp a send
/// records is the database clock's.
const NOW: Timestamp = Timestamp::constant(1_767_225_600, 0);

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_email_attempt_is_counted_named_and_recorded_while_latest() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite("Bob@Example.test", NOW)
        .await
        .expect("the invite issues");

    // The first send is attempt 1 and names the inviter and the account.
    let first = fixture
        .begin_email(&invite)
        .await
        .expect("the invite is still pending");
    assert_eq!(first.attempt, 1);
    assert_eq!(first.to, "bob@example.test");
    assert_eq!(first.inviter_name, fixture.john.name);
    assert_eq!(first.owner_name, fixture.john.name);

    // A second send begins before the first records: the first's result is
    // stale and must not land; the second's does.
    let second = fixture
        .begin_email(&invite)
        .await
        .expect("the invite is still pending");
    assert_eq!(second.attempt, 2);
    let stamped = fixture
        .team
        .record_email(&invite, second.attempt, EmailStatus::Sent)
        .await
        .expect("the latest attempt's result is recorded");
    let stale = fixture
        .team
        .record_email(&invite, first.attempt, EmailStatus::Failed)
        .await
        .expect("a stale result is answered, not refused");
    assert_eq!(stale, None, "a stale attempt records nothing");
    let (status, sent_at) = email_record(&fixture, &invite).await;
    assert_eq!(status, EmailStatus::Sent, "the stale failure did not land");
    assert!(sent_at.is_some(), "the delivery is stamped");
    assert_eq!(sent_at, stamped, "the answer is the stored stamp");

    nothing_counts_but_the_pending_invite_of_its_own_account(&fixture, &invite).await;
    fixture.cleanup().await;
}

/// Another account cannot count a send of John's invite, nor can anyone
/// count one for a revoked invite.
async fn nothing_counts_but_the_pending_invite_of_its_own_account(
    fixture: &Fixture,
    invite: &Uuid7,
) {
    let (tenant, other) = (id(&fixture.john.tenant), id(&fixture.carol.tenant));
    let foreign = fixture.team.begin_email(&other, invite).await;
    assert!(
        foreign.expect("another account's send answers").is_none(),
        "another account counts nothing"
    );
    fixture
        .team
        .revoke_invitation(&tenant, invite)
        .await
        .expect("the revoke lands");
    assert_eq!(
        fixture.begin_email(invite).await,
        None,
        "a revoked invite counts nothing"
    );
}

/// The email record's edges: a send that began and never recorded reads as
/// failed; a later failed attempt keeps the earlier delivery's instant; an
/// accepted invite has nothing left to send.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_read_failed_when_attempt_began_and_never_recorded() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");

    fixture
        .begin_email(&invite)
        .await
        .expect("the invite is still pending");
    assert_eq!(
        email_record(&fixture, &invite).await,
        (EmailStatus::Failed, None),
        "begun, never recorded"
    );

    send(&fixture, &invite, EmailStatus::Sent).await;
    let (_, delivered) = email_record(&fixture, &invite).await;
    assert!(delivered.is_some(), "the delivery is stamped");
    send(&fixture, &invite, EmailStatus::Failed).await;
    assert_eq!(
        email_record(&fixture, &invite).await,
        (EmailStatus::Failed, delivered),
        "the last delivery's instant survives a later failure"
    );

    fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect("the accept lands");
    assert_eq!(
        fixture.begin_email(&invite).await,
        None,
        "an accepted invite has nothing to send"
    );
    fixture.cleanup().await;
}

/// One send of `invite`: counted, then recorded as `status`.
async fn send(fixture: &Fixture, invite: &Uuid7, status: EmailStatus) {
    let attempt = fixture
        .begin_email(invite)
        .await
        .expect("the invite is still pending");
    fixture
        .team
        .record_email(invite, attempt.attempt, status)
        .await
        .expect("the send's result is recorded");
}

/// What John's invite list says of `invite`'s email.
async fn email_record(fixture: &Fixture, invite: &Uuid7) -> (EmailStatus, Option<Timestamp>) {
    let listed = fixture.johns_invites().await;
    let row = listed
        .iter()
        .find(|row| row.id == *invite)
        .expect("the invite is listed");
    (row.email_status, row.email_sent_at)
}
