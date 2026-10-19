//! An owner's invites against live Postgres, past the spec's Dimensions: a
//! revoke scoped to its account, an invite closed once past its expiry, the
//! lists that drop a closed invite, a revoke that lost to a join, and an
//! accept that fails whole.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_core::error_code;
use afd_tenant::error::InviteConflict;
use jiff::Timestamp;

use crate::access_lane::{Fixture, id};

/// The instant each identifier a case mints is minted at. Every stamp and
/// deadline is the database clock's.
const NOW: Timestamp = Timestamp::constant(1_767_225_600, 0);

/// How far past its deadline a backdated invite sits.
const LAPSED: Duration = Duration::from_millis(1);

/// Another account naming John's invite revokes nothing: the revoke is scoped
/// to the account that issued it, so John's invite stays listed and Carol can
/// still accept it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_leave_invite_pending_when_another_account_revokes_it() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let stranger = id(&fixture.bob.tenant);
    fixture
        .team
        .revoke_invitation(&stranger, &invite)
        .await
        .expect("a revoke of nothing of yours is quiet");

    let listed = fixture
        .team
        .invitations(&id(&fixture.john.tenant))
        .await
        .expect("the list reads");
    assert!(listed.iter().any(|row| row.id == invite), "still pending");
    fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect("Carol can still accept");
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 1);
    fixture.cleanup().await;
}

/// Once the database clock passes an invite's deadline it is closed
/// everywhere at once: the list, the invitee's waiting list, a send, an
/// accept, and a fresh invite to the same address retires it as expired.
/// Before that it is open. The exact instant is the unit suite's
/// (`expired_revoked_and_someone_elses_are_one_answer`): a live clock cannot
/// be stopped on it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_treat_invite_as_closed_everywhere_once_past_expiry() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    assert!(fixture.johns_lists(&invite).await, "open inside its window");

    fixture.backdate_expiry(&invite, LAPSED).await;
    assert!(!fixture.johns_lists(&invite).await, "closed in the list");
    let waiting = fixture.waiting_for(&fixture.carol.email).await;
    assert!(
        !waiting.iter().any(|row| row.id == invite),
        "closed for Carol"
    );
    assert_eq!(fixture.begin_email(&invite).await, None, "nothing to send");
    let refused = fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect_err("closed to accept");
    assert_eq!(refused.code(), error_code::INVITE_NOT_FOUND);
    fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("a fresh invite supersedes the closed one");
    assert_eq!(
        fixture.stamps(&invite).await,
        ("expired".to_owned(), true),
        "the fresh invite retired the lapsed one as expired"
    );
    fixture.cleanup().await;
}

/// The invitee's waiting list matches their address in any case and drops an
/// invite once it is revoked; the owner's list drops it once it is accepted.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_list_only_open_invites_when_revoked_or_accepted() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let shouted = fixture.carol.email.to_uppercase();
    let waiting = fixture.waiting_for(&shouted).await;
    let found = waiting
        .iter()
        .find(|row| row.id == invite)
        .expect("found in any case");
    assert_eq!(found.tenant, tenant);

    fixture
        .team
        .revoke_invitation(&tenant, &invite)
        .await
        .expect("the revoke lands");
    assert_eq!(fixture.stamps(&invite).await, ("revoked".to_owned(), true));
    let waiting = fixture.waiting_for(&fixture.carol.email).await;
    assert!(
        !waiting.iter().any(|row| row.id == invite),
        "revoked is gone"
    );

    let second = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("a fresh invite");
    fixture
        .team
        .accept(&second, &fixture.carol.invitee(), NOW)
        .await
        .expect("the accept lands");
    assert!(!fixture.johns_lists(&second).await, "accepted is gone");
    fixture.cleanup().await;
}

/// John revokes an invite Carol already joined through: he is told she is a
/// member rather than that she was kept out, and the invite stays accepted.
/// Once she has left, revoking the spent invite is quiet again.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_refuse_revoking_an_invite_its_invitee_joined_through() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect("Carol joins");

    let refused = fixture
        .team
        .revoke_invitation(&tenant, &invite)
        .await
        .expect_err("Carol already joined");
    assert_eq!(refused.code(), error_code::INVITE_CONFLICT);
    assert_eq!(refused.invite_conflict(), Some(InviteConflict::Member));
    assert_eq!(
        fixture.stamps(&invite).await,
        ("accepted".to_owned(), true),
        "the refused revoke changed nothing"
    );
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 1);

    fixture
        .team
        .remove(&tenant, &fixture.carol.user_id)
        .await
        .expect("John removes Carol");
    fixture
        .team
        .revoke_invitation(&tenant, &invite)
        .await
        .expect("a spent invite whose invitee left revokes quietly");
    fixture.cleanup().await;
}

/// Another account naming an invite Carol already joined through learns
/// nothing: its revoke answers as a revoke of nothing does, and John's invite
/// and Carol's membership are untouched.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_answer_quietly_when_another_account_revokes_an_accepted_invite() {
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

    let stranger = id(&fixture.bob.tenant);
    fixture
        .team
        .revoke_invitation(&stranger, &invite)
        .await
        .expect("a revoke of nothing of yours is quiet, accepted or not");
    assert_eq!(fixture.stamps(&invite).await, ("accepted".to_owned(), true));
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 1);
    fixture.cleanup().await;
}

/// A workspace row this daemon cannot read fails the accept before it
/// commits: Carol is not left a member whose every accept answers 500, the
/// invite stays open, and once the row is gone the same accept lands.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_roll_back_the_join_when_a_workspace_row_cannot_be_read() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let unreadable = fixture.add_unreadable_workspace().await;

    let refused = fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect_err("an unreadable workspace fails the accept");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 0);
    assert_eq!(
        fixture.stamps(&invite).await,
        ("pending".to_owned(), false),
        "still open"
    );

    fixture.remove_workspace(&unreadable).await;
    let accepted = fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect("the retry lands once the row is readable");
    assert_eq!(accepted.workspaces, [id(&fixture.john.workspace)]);
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 1);
    fixture.cleanup().await;
}
