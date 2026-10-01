//! An account's members against live Postgres: who already belongs to it, an
//! owner removed while another stays, a role an accept does not overwrite,
//! and an insert failure that is not a conflict.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_db::test_util::mint_id;
use afd_tenant::error::InviteConflict;
use afd_tenant::team::{Email, NewInvite, Removal};
use afd_tenant::test_util::hold;
use afd_tenant::workspace::access::ROLE_OWNER;

use crate::access_lane::{Fixture, id};

/// Now, for every call a case makes.
const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// An account's own user holds it as owner even without a membership row, so
/// inviting their address is refused as a member's; an accepted invite would
/// write a member row and demote them in their own account.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_refuse_inviting_an_owner_no_membership_row_backs() {
    let fixture = Fixture::create().await;
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query("DELETE FROM core.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid")
        .bind(&fixture.john.tenant)
        .bind(&fixture.john.user)
        .execute(&mut *connection)
        .await
        .expect("John's account predates memberships");
    drop(connection);
    let refused = fixture
        .invite(&fixture.john.email, NOW)
        .await
        .expect_err("John already holds his account");
    assert_eq!(refused.code(), error_code::INVITE_CONFLICT);
    assert_eq!(refused.invite_conflict(), Some(InviteConflict::Member));
    fixture.cleanup().await;
}

/// An owner can be removed while another owner stays; the one left is then
/// the last owner.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_remove_owner_when_another_owner_remains() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    hold(
        &fixture.database,
        &fixture.john.tenant,
        &fixture.carol.user,
        ROLE_OWNER,
    )
    .await;

    let removed = fixture
        .team
        .remove(&tenant, &fixture.carol.user_id)
        .await
        .expect("a second owner can go");
    assert_eq!(removed, Removal::Removed);
    assert_eq!(fixture.owners_in_johns().await, 1);
    let last = fixture
        .team
        .remove(&tenant, &fixture.john.user_id)
        .await
        .expect_err("John is now the last owner");
    assert_eq!(last.code(), error_code::MEMBER_LAST_OWNER);
    fixture.cleanup().await;
}

/// Someone who joined another way before accepting keeps the role they hold:
/// the accept stamps the invite and grants nothing over it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_keep_existing_role_when_member_accepts_invite() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    hold(
        &fixture.database,
        &fixture.john.tenant,
        &fixture.carol.user,
        ROLE_OWNER,
    )
    .await;

    let accepted = fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect("the accept answers");
    assert!(accepted.workspaces.contains(&id(&fixture.john.workspace)));
    assert_eq!(
        fixture.role_in_johns(&fixture.carol).await.as_deref(),
        Some(ROLE_OWNER)
    );
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 1);
    let listed = fixture
        .team
        .invitations(&id(&fixture.john.tenant), NOW)
        .await
        .expect("the invite list reads");
    assert!(
        !listed.iter().any(|row| row.id == invite),
        "the invite is spent"
    );
    fixture.cleanup().await;
}

/// An invite into an account that does not exist is a datastore refusal the
/// unique-index arm does not swallow as a conflict.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_query_failure_when_insert_fails_other_than_unique() {
    let fixture = Fixture::create().await;
    let nowhere = id(&mint_id());
    let email = Email::parse(&fixture.carol.email, |_| true).expect("an address");
    let refused = fixture
        .team
        .invite(
            &NewInvite {
                tenant: &nowhere,
                inviter: &fixture.john.user_id,
                email: &email,
            },
            NOW,
        )
        .await
        .expect_err("no such account");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    fixture.cleanup().await;
}
