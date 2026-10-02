//! The access decision through memberships, against live Postgres.
//!
//! John owns an account. Bob owns his own and is a member of John's; Carol
//! owns hers and holds nothing of John's. The resolver is asked what each may
//! open and which accounts each holds, from the rows alone. The walk across
//! those accounts is `integration_workspace_directory.rs`; how the answer
//! reaches a route is `afd_api`'s concern.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_auth::principal::PersonCredential;
use afd_auth::scope::ScopeSet;
use afd_core::error_code;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_tenant::workspace::Workspaces;
use afd_tenant::workspace::access::Role;

use crate::access_lane::{Account, Fixture, held, id, person, session};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn membership_decides_access_and_the_role_it_is_held_with() {
    let fixture = Fixture::create().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let (john, bob, stranger) = (&fixture.john, &fixture.bob, &fixture.carol);
    let bob_session = session(&bob.tenant, &bob.subject);

    let reached = workspaces
        .authorize(&bob_session, &id(&john.workspace))
        .await
        .expect("the access check answers");
    assert_eq!(reached, Some(held(&john.tenant, Role::Member)));
    let own = workspaces
        .authorize(&bob_session, &id(&bob.workspace))
        .await
        .expect("the access check answers");
    assert_eq!(
        own,
        Some(held(&bob.tenant, Role::Owner)),
        "Bob still owns his own"
    );

    let outsider = session(&stranger.tenant, &stranger.subject);
    let refused = workspaces
        .authorize(&outsider, &id(&john.workspace))
        .await
        .expect("the access check answers");
    assert_eq!(refused, None, "no membership, no access");

    // Bob's own api-key names his account and acts only there, even though
    // the person who minted it is a member of John's.
    let key = person(
        PersonCredential::TenantApiKey,
        &bob.tenant,
        &bob.subject,
        ScopeSet::EMPTY,
    );
    let by_key = workspaces
        .authorize(&key, &id(&john.workspace))
        .await
        .expect("the access check answers");
    assert_eq!(
        by_key, None,
        "a claim-bound credential reaches nothing but its own account"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stored_role_this_build_does_not_know_is_reported_not_guessed() {
    let fixture = Fixture::create().await;
    fixture.set_bob_in_johns_account("viewer").await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());

    let refused = workspaces
        .authorize(
            &session(&fixture.bob.tenant, &fixture.bob.subject),
            &id(&fixture.john.workspace),
        )
        .await
        .expect_err("an unreadable role is a fault, never a verdict");

    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_accounts_a_person_holds_are_their_own_and_every_membership() {
    let fixture = Fixture::create().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let (john, bob) = (&fixture.john, &fixture.bob);

    let bob_session = session(&bob.tenant, &bob.subject);
    let accounts = workspaces
        .accounts_of(bob_session.person().expect("a session is a person"))
        .await
        .expect("the account read answers");
    assert_eq!(accounts.home, id(&bob.tenant));
    assert_eq!(accounts.held.len(), 2, "{accounts:?}");
    let johns = accounts
        .get(&id(&john.tenant))
        .expect("John's account is held");
    assert_eq!(
        (johns.role, johns.owner_name.as_str()),
        (Role::Member, "John")
    );
    let own = accounts
        .get(&id(&bob.tenant))
        .expect("Bob's own account is held");
    assert_eq!(own.role, Role::Owner);

    let page = workspaces
        .page(&accounts.tenants(), None, None, 50)
        .await
        .expect("the page reads");
    let tenants: Vec<&str> = page.rows.iter().map(|row| row.tenant_id.as_str()).collect();
    assert!(tenants.contains(&john.tenant.as_str()), "{tenants:?}");
    assert!(tenants.contains(&bob.tenant.as_str()), "{tenants:?}");
    an_api_key_holds_only_its_own_account(&workspaces, bob).await;
    fixture.cleanup().await;
}

/// Bob's api-key names his account and holds only it, though the person who
/// minted it is a member of John's too.
async fn an_api_key_holds_only_its_own_account(workspaces: &Workspaces, bob: &Account) {
    let key = person(
        PersonCredential::TenantApiKey,
        &bob.tenant,
        &bob.subject,
        ScopeSet::EMPTY,
    );
    let by_key = workspaces
        .accounts_of(key.person().expect("a tenant api-key acts for a person"))
        .await
        .expect("the account read answers");
    assert_eq!(
        by_key.held.len(),
        1,
        "an api-key holds only the account it was minted in"
    );
    assert!(
        by_key
            .get(&id(&bob.tenant))
            .is_some_and(|account| account.role == Role::Owner)
    );
}

/// A session whose subject has no user row falls back to the account its
/// claim names, held as owner; a claim naming no account holds nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_fall_back_to_claim_when_session_subject_has_no_user_row() {
    let fixture = Fixture::create().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let unknown = format!("user_unknown_{}", mint_id());

    let claim = session(&fixture.john.tenant, &unknown);
    let claimed = workspaces
        .accounts_of(claim.person().expect("a session is a person"))
        .await
        .expect("the account read answers");
    assert_eq!(claimed.home, id(&fixture.john.tenant));
    assert_eq!(claimed.held.len(), 1, "{claimed:?}");
    assert!(
        claimed
            .get(&id(&fixture.john.tenant))
            .is_some_and(|account| account.role == Role::Owner)
    );

    let nowhere = mint_id();
    let nowhere_claim = session(&nowhere, &unknown);
    let empty = workspaces
        .accounts_of(nowhere_claim.person().expect("a session is a person"))
        .await
        .expect("the account read answers");
    assert_eq!(empty.home, id(&nowhere));
    assert!(empty.held.is_empty(), "{empty:?}");
    fixture.cleanup().await;
}
