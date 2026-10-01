//! The invite email's store half against live Postgres: an attempt is counted
//! before its send and names what the email says, and a result is recorded
//! only while its attempt is the latest.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::team::{Email, EmailStatus, NewInvite, Team};

use crate::access_lane::{Signup, delete_accounts, id, sign_up};

const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);
const LATER: UnixMillis = UnixMillis::from_millis(1_767_225_660_000);
const OWNER_NAME: &str = "John Owner";

/// One signed-up person: tenant, user, address.
fn minted(name: &str) -> (String, String, String) {
    (
        mint_id(),
        mint_id(),
        format!("{name}+{}@example.test", mint_id()),
    )
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_email_attempt_is_counted_named_and_recorded_while_latest() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let team = Team::new(database.clone(), Entropy::new());
    let (tenant, user, address) = minted("john");
    let (other_tenant, other_user, other_address) = minted("eve");
    for (tenant, user, address, display_name) in [
        (&tenant, &user, &address, Some(OWNER_NAME)),
        (&other_tenant, &other_user, &other_address, None),
    ] {
        let subject = format!("user_mail_{}", mint_id());
        let workspace = mint_id();
        let signup = Signup {
            tenant,
            user,
            subject: &subject,
            email: address,
            name: "mail",
            display_name,
            workspace: &workspace,
        };
        sign_up(&database, &signup).await;
    }

    let tenant_id = id(&tenant);
    let inviter = id(&user);
    let email = Email::parse("Bob@Example.test", |_| true).expect("an address");
    let invite = team
        .invite(
            &NewInvite {
                tenant: &tenant_id,
                inviter: &inviter,
                email: &email,
            },
            NOW,
        )
        .await
        .expect("the invite issues")
        .id;

    // The first send is attempt 1 and names the inviter and the account.
    let first = team
        .begin_email(&tenant_id, &invite, NOW)
        .await
        .expect("begins")
        .expect("pending");
    assert_eq!(first.attempt, 1);
    assert_eq!(first.to, "bob@example.test");
    assert_eq!(first.inviter_name, OWNER_NAME);
    assert_eq!(first.owner_name, OWNER_NAME);

    // A second send begins before the first records: the first's result is
    // stale and must not land; the second's does.
    let second = team
        .begin_email(&tenant_id, &invite, LATER)
        .await
        .expect("begins")
        .expect("pending");
    assert_eq!(second.attempt, 2);
    team.record_email(&invite, second.attempt, EmailStatus::Sent, LATER)
        .await
        .expect("records");
    team.record_email(&invite, first.attempt, EmailStatus::Failed, LATER)
        .await
        .expect("records");
    let listed = team.invitations(&tenant_id, LATER).await.expect("lists");
    let row = listed
        .iter()
        .find(|row| row.id == invite)
        .expect("the invite is listed");
    assert_eq!(row.email_status, EmailStatus::Sent);
    assert_eq!(row.email_sent_at_ms, Some(LATER.as_millis()));

    // Another account cannot count a send of John's invite, nor can anyone
    // count one for a revoked invite.
    let other = id(&other_tenant);
    assert!(
        team.begin_email(&other, &invite, LATER)
            .await
            .expect("answers")
            .is_none()
    );
    team.revoke_invitation(&tenant_id, &invite, LATER)
        .await
        .expect("revokes");
    assert!(
        team.begin_email(&tenant_id, &invite, LATER)
            .await
            .expect("answers")
            .is_none()
    );

    let third = mint_id();
    delete_accounts(&database, [&tenant, &other_tenant, &third]).await;
    drop(database);
    lane.cleanup().await;
}
