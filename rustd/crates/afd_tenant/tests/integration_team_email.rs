//! The invite email's store half against live Postgres: an attempt is counted
//! before its send and names what the email says, a result is recorded only
//! while its attempt is the latest, and a send that never recorded reads as
//! failed.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_tenant::team::EmailStatus;

use crate::access_lane::{Fixture, id};

const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);
const LATER: UnixMillis = UnixMillis::from_millis(1_767_225_660_000);

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
        .begin_email(&invite, NOW)
        .await
        .expect("the invite is still pending");
    assert_eq!(first.attempt, 1);
    assert_eq!(first.to, "bob@example.test");
    assert_eq!(first.inviter_name, fixture.john.name);
    assert_eq!(first.owner_name, fixture.john.name);

    // A second send begins before the first records: the first's result is
    // stale and must not land; the second's does.
    let second = fixture
        .begin_email(&invite, LATER)
        .await
        .expect("the invite is still pending");
    assert_eq!(second.attempt, 2);
    for (attempt, status) in [
        (second.attempt, EmailStatus::Sent),
        (first.attempt, EmailStatus::Failed),
    ] {
        let recorded = fixture.team.record_email(&invite, attempt, status, LATER);
        recorded.await.expect("a result is answered, stale or not");
    }
    assert_eq!(
        email_record(&fixture, &invite, LATER).await,
        (EmailStatus::Sent, Some(LATER.as_millis()))
    );

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
    let foreign = fixture.team.begin_email(&other, invite, LATER).await;
    assert!(
        foreign.expect("another account's send answers").is_none(),
        "another account counts nothing"
    );
    fixture
        .team
        .revoke_invitation(&tenant, invite, LATER)
        .await
        .expect("the revoke lands");
    assert_eq!(
        fixture.begin_email(invite, LATER).await,
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
    let later = UnixMillis::from_millis(NOW.as_millis() + 1);
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");

    fixture
        .begin_email(&invite, NOW)
        .await
        .expect("the invite is still pending");
    assert_eq!(
        email_record(&fixture, &invite, NOW).await,
        (EmailStatus::Failed, None),
        "begun, never recorded"
    );

    send(&fixture, &invite, NOW, EmailStatus::Sent).await;
    send(&fixture, &invite, later, EmailStatus::Failed).await;
    assert_eq!(
        email_record(&fixture, &invite, later).await,
        (EmailStatus::Failed, Some(NOW.as_millis())),
        "the last delivery's instant survives a later failure"
    );

    fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), later)
        .await
        .expect("the accept lands");
    assert_eq!(
        fixture.begin_email(&invite, later).await,
        None,
        "an accepted invite has nothing to send"
    );
    fixture.cleanup().await;
}

/// One send of `invite` at `at`: counted, then recorded as `status`.
async fn send(fixture: &Fixture, invite: &Uuid7, at: UnixMillis, status: EmailStatus) {
    let attempt = fixture
        .begin_email(invite, at)
        .await
        .expect("the invite is still pending");
    fixture
        .team
        .record_email(invite, attempt.attempt, status, at)
        .await
        .expect("the send's result is recorded");
}

/// What John's invite list says of `invite`'s email at `at`.
async fn email_record(
    fixture: &Fixture,
    invite: &Uuid7,
    at: UnixMillis,
) -> (EmailStatus, Option<i64>) {
    let listed = fixture.johns_invites(at).await;
    let row = listed
        .iter()
        .find(|row| row.id == *invite)
        .expect("the invite is listed");
    (row.email_status, row.email_sent_at_ms)
}
