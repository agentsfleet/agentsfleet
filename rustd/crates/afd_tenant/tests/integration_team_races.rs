//! The team store's races against live Postgres: two owners removing each
//! other, an accept against a revoke, an accept against a fresh invite, and two
//! invites to one address. Each
//! runs a dozen rounds, since a lost lock order shows in most but not all.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_db::test_util::mint_id;
use afd_tenant::error::InviteConflict;
use afd_tenant::team::Removal;
use afd_tenant::test_util::hold;
use afd_tenant::workspace::access::ROLE_OWNER;

use crate::access_lane::{Fixture, id};

/// Now, for every call a case makes.
const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// How many times each race runs. Two concurrent removals that lock in
/// different orders deadlock in most rounds, so a dozen shows it.
const RACE_ROUNDS: usize = 12;

/// Two owners removing each other at once: every round, one goes and the
/// other is told it is the last owner. Locking the target's row before the
/// owners let each removal hold its own row and wait on the other's, and
/// Postgres broke the deadlock by aborting one, which answered as a datastore
/// failure.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_keep_one_owner_when_two_owners_remove_each_other_concurrently() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    for round in 0..RACE_ROUNDS {
        for owner in [&fixture.john, &fixture.carol] {
            hold(
                &fixture.database,
                &fixture.john.tenant,
                &owner.user,
                ROLE_OWNER,
            )
            .await;
        }
        let (john, carol) = tokio::join!(
            fixture.team.remove(&tenant, &fixture.john.user_id),
            fixture.team.remove(&tenant, &fixture.carol.user_id),
        );
        let outcomes = [john, carol];
        let removed = outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Ok(Removal::Removed)))
            .count();
        let refused: Vec<_> = outcomes
            .iter()
            .filter_map(|outcome| outcome.as_ref().err())
            .map(afd_tenant::Error::code)
            .collect();
        assert_eq!(removed, 1, "round {round}: exactly one owner goes");
        assert_eq!(
            refused,
            [error_code::MEMBER_LAST_OWNER],
            "round {round}: the other is the last owner, not a datastore failure"
        );
        assert_eq!(fixture.owners_in_johns().await, 1, "round {round}");
    }
    fixture.cleanup().await;
}

/// An accept racing a revoke settles one way: Carol joins, the invite is
/// spent and the revoke says she is a member; or the revoke wins and she is
/// told the invite is gone, with no membership. Never a membership from a
/// revoked invite, never a refusal after a join, and never a revoke that
/// answers success while she holds the account.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_settle_one_outcome_when_accept_races_revoke() {
    let fixture = Fixture::create().await;
    for round in 0..RACE_ROUNDS {
        accept_against_revoke(&fixture, round).await;
    }
    fixture.cleanup().await;
}

/// One round: Carol accepts while John revokes, and both answers agree with
/// the rows they left.
async fn accept_against_revoke(fixture: &Fixture, round: usize) {
    let tenant = id(&fixture.john.tenant);
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let carol = fixture.carol.invitee();
    let (accepted, revoked) = tokio::join!(
        fixture.team.accept(&invite, &carol, NOW),
        fixture.team.revoke_invitation(&tenant, &invite, NOW),
    );
    let joined = fixture.memberships_in_johns(&fixture.carol).await;
    let (accepted_at, revoked_at) = fixture.stamps(&invite).await;
    assert!(
        accepted_at.is_none() || revoked_at.is_none(),
        "round {round}: an invite is accepted or revoked, never both"
    );
    assert_eq!(
        joined == 1,
        accepted_at.is_some() && revoked_at.is_none(),
        "round {round}: a membership only from an accepted, unrevoked invite"
    );
    let revoke_refused = revoked.err().map(|refusal| refusal.invite_conflict());
    match accepted {
        Ok(_) => {
            assert_eq!(joined, 1, "round {round}: an accept that lands joins");
            assert_eq!(
                revoke_refused,
                Some(Some(InviteConflict::Member)),
                "round {round}: the revoke that lost says Carol joined"
            );
            fixture
                .team
                .remove(&tenant, &fixture.carol.user_id)
                .await
                .expect("Carol leaves for the next round");
        }
        Err(refusal) => {
            let code = refusal.code();
            assert_eq!(code, error_code::INVITE_NOT_FOUND, "round {round}");
            assert_eq!(joined, 0, "round {round}: a refused accept joins nothing");
            assert_eq!(
                revoke_refused, None,
                "round {round}: the revoke that won answers success"
            );
        }
    }
}

/// An accept racing a new invite to the same address: Carol joins, and the
/// invite is refused as a conflict. Never a pending invite issued, and
/// emailed, to someone already in the account. The member check used to read
/// before the accept committed, then the accept cleared the one-pending-invite
/// index and the new invite slipped in.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_refuse_new_invite_when_it_races_an_accept_for_the_same_address() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    let carol = fixture.carol.invitee();
    for round in 0..RACE_ROUNDS {
        let earlier = fixture
            .invite(&fixture.carol.email, NOW)
            .await
            .expect("John invites Carol");
        let (accepted, issued) = tokio::join!(
            fixture.team.accept(&earlier, &carol, NOW),
            fixture.invite(&fixture.carol.email, NOW),
        );
        accepted.expect("Carol's accept of the earlier invite lands");
        let refusal = issued.expect_err("no new invite for a member");
        assert_eq!(refusal.code(), error_code::INVITE_CONFLICT, "round {round}");
        assert!(
            fixture.johns_invites(NOW).await.is_empty(),
            "round {round}: nothing left pending for Carol"
        );
        fixture
            .team
            .remove(&tenant, &fixture.carol.user_id)
            .await
            .expect("Carol leaves for the next round");
    }
    fixture.cleanup().await;
}

/// Two invites to one address at once: one issues, the other is the conflict,
/// and neither is a datastore failure.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_issue_one_invite_when_two_owners_invite_same_address_concurrently() {
    let fixture = Fixture::create().await;
    for round in 0..RACE_ROUNDS {
        let address = format!("dave+{}@example.test", mint_id());
        let (first, second) =
            tokio::join!(fixture.invite(&address, NOW), fixture.invite(&address, NOW),);
        let outcomes = [first, second];
        let issued = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
        let refused: Vec<_> = outcomes
            .iter()
            .filter_map(|outcome| outcome.as_ref().err())
            .map(afd_tenant::Error::code)
            .collect();
        assert_eq!(issued, 1, "round {round}: one invite issues");
        assert_eq!(refused, [error_code::INVITE_CONFLICT], "round {round}");
    }
    fixture.cleanup().await;
}
