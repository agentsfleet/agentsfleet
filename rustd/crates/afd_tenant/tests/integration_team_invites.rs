//! An owner's invites against live Postgres, past the spec's Dimensions: a
//! revoke scoped to its account, an invite closed at its expiry instant, the
//! lists that drop a closed invite, and a revoke that lost to a join.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_tenant::error::InviteConflict;
use afd_tenant::team::INVITE_TTL_MS;

use crate::access_lane::{Fixture, id};

/// Now, for every call a case makes.
const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

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
        .revoke_invitation(&stranger, &invite, NOW)
        .await
        .expect("a revoke of nothing of yours is quiet");

    let listed = fixture
        .team
        .invitations(&id(&fixture.john.tenant), NOW)
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

/// The instant an invite expires it is closed everywhere at once: the list,
/// the invitee's waiting list, a send, an accept, and a fresh invite to the
/// same address supersedes it. One millisecond earlier it is still open.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_treat_invite_as_closed_everywhere_when_now_equals_expiry() {
    let fixture = Fixture::create().await;
    let issued = UnixMillis::from_millis(NOW.as_millis() - INVITE_TTL_MS);
    let just_before = UnixMillis::from_millis(NOW.as_millis() - 1);
    let invite = fixture
        .invite(&fixture.carol.email, issued)
        .await
        .expect("an invite that expires at NOW");

    assert!(
        fixture.johns_lists(&invite, just_before).await,
        "open a millisecond early"
    );
    assert!(
        !fixture.johns_lists(&invite, NOW).await,
        "closed in the list"
    );
    let waiting = fixture.waiting_for(&fixture.carol.email, NOW).await;
    assert!(
        !waiting.iter().any(|row| row.id == invite),
        "closed for Carol"
    );
    assert_eq!(
        fixture.begin_email(&invite, NOW).await,
        None,
        "nothing to send"
    );
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
    let waiting = fixture.waiting_for(&shouted, NOW).await;
    let found = waiting
        .iter()
        .find(|row| row.id == invite)
        .expect("found in any case");
    assert_eq!(found.tenant, tenant);

    fixture
        .team
        .revoke_invitation(&tenant, &invite, NOW)
        .await
        .expect("the revoke lands");
    let waiting = fixture.waiting_for(&fixture.carol.email, NOW).await;
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
    assert!(!fixture.johns_lists(&second, NOW).await, "accepted is gone");
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
        .revoke_invitation(&tenant, &invite, NOW)
        .await
        .expect_err("Carol already joined");
    assert_eq!(refused.code(), error_code::INVITE_CONFLICT);
    assert_eq!(refused.invite_conflict(), Some(InviteConflict::Member));
    let (accepted_at, revoked_at) = fixture.stamps(&invite).await;
    assert_eq!(
        (accepted_at.is_some(), revoked_at),
        (true, None),
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
        .revoke_invitation(&tenant, &invite, NOW)
        .await
        .expect("a spent invite whose invitee left revokes quietly");
    fixture.cleanup().await;
}
