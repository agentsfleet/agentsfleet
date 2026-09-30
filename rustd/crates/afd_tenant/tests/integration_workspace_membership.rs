//! The access decision through memberships, against live Postgres.
//!
//! John owns an account. Bob owns his own and is a member of John's. The
//! resolver is asked what each may open and which accounts each holds, from the
//! rows alone. How the answer reaches a route is `afd_api`'s concern.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_auth::principal::PersonCredential;
use afd_auth::scope::ScopeSet;
use afd_core::error_code;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::workspace::Workspaces;
use afd_tenant::workspace::access::{ROLE_MEMBER, Role};

use crate::access_lane::{Signup, delete_accounts, held, hold, id, person, session, sign_up};

/// One signed-up person: their account, user row and workspace.
struct Owner {
    tenant: String,
    user: String,
    subject: String,
    workspace: String,
}

impl Owner {
    fn minted() -> Self {
        Self {
            tenant: mint_id(),
            user: mint_id(),
            subject: format!("user_membership_{}", mint_id()),
            workspace: mint_id(),
        }
    }
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    john: Owner,
    bob: Owner,
    stranger: Owner,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            john: Owner::minted(),
            bob: Owner::minted(),
            stranger: Owner::minted(),
            lane,
        }
    }

    async fn seed(&self) {
        for (owner, name) in [
            (&self.john, "John"),
            (&self.bob, "Bob"),
            (&self.stranger, "Stranger"),
        ] {
            self.sign_up(owner, name).await;
        }
        self.set_bob_in_johns_account(ROLE_MEMBER).await;
    }

    async fn sign_up(&self, owner: &Owner, display_name: &str) {
        let person = Signup {
            tenant: &owner.tenant,
            user: &owner.user,
            subject: &owner.subject,
            email: "fixture@example.test",
            name: display_name,
            display_name: Some(display_name),
            workspace: &owner.workspace,
        };
        sign_up(&self.database, &person).await;
    }

    /// Bob's row in John's account, holding `role` whatever it held before.
    async fn set_bob_in_johns_account(&self, role: &str) {
        hold(&self.database, &self.john.tenant, &self.bob.user, role).await;
    }

    async fn cleanup(self) {
        let accounts = [&*self.john.tenant, &self.bob.tenant, &self.stranger.tenant];
        delete_accounts(&self.database, accounts).await;
        drop(self.database);
        self.lane.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn membership_decides_access_and_the_role_it_is_held_with() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let (john, bob, stranger) = (&fixture.john, &fixture.bob, &fixture.stranger);
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
    fixture.seed().await;
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
    fixture.seed().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let (john, bob) = (&fixture.john, &fixture.bob);

    let accounts = workspaces
        .accounts_of(&session(&bob.tenant, &bob.subject))
        .await
        .expect("the account read answers")
        .expect("a person holds accounts");
    assert_eq!(accounts.home, id(&bob.tenant));
    assert_eq!(accounts.held.len(), 2, "{accounts:?}");
    let johns = accounts.get(&john.tenant).expect("John's account is held");
    assert_eq!(
        (johns.role, johns.owner_name.as_str()),
        (Role::Member, "John")
    );
    let own = accounts
        .get(&bob.tenant)
        .expect("Bob's own account is held");
    assert_eq!(own.role, Role::Owner);

    let page = workspaces
        .page(&accounts.tenants(), None, None, 50)
        .await
        .expect("the page reads");
    let tenants: Vec<&str> = page.rows.iter().map(|row| row.tenant_id.as_str()).collect();
    assert!(tenants.contains(&john.tenant.as_str()), "{tenants:?}");
    assert!(tenants.contains(&bob.tenant.as_str()), "{tenants:?}");

    let key = person(
        PersonCredential::TenantApiKey,
        &bob.tenant,
        &bob.subject,
        ScopeSet::EMPTY,
    );
    let by_key = workspaces
        .accounts_of(&key)
        .await
        .expect("the account read answers")
        .expect("a key holds its own account");
    assert_eq!(
        by_key.held.len(),
        1,
        "an api-key holds only the account it was minted in"
    );
    assert!(
        by_key
            .get(&bob.tenant)
            .is_some_and(|account| account.role == Role::Owner)
    );

    fixture.cleanup().await;
}
