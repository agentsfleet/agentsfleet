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

use afd_auth::principal::{PersonCredential, Principal, Runner};
use afd_auth::scope::ScopeSet;
use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::workspace::Workspaces;
use afd_tenant::workspace::access::{ROLE_MEMBER, Role};
use afd_tenant::workspace::directory::After;
use afd_tenant::workspace::name::Chosen;

use crate::access_lane::{Signup, delete_accounts, held, hold, id, person, session, sign_up};

/// A name both accounts give a workspace, so a name filter must keep them apart.
const SHARED_NAME: &str = "shared-across-accounts";

/// When the extra workspaces are made: after every signup's own.
const LATER: UnixMillis = UnixMillis::from_millis(4_102_444_800_000);

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

/// A runner acts for no person and holds no account.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_answer_none_when_principal_is_a_runner() {
    let fixture = Fixture::create().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let runner = Principal::Runner(Runner::new(id(&mint_id()), false));
    let accounts = workspaces
        .accounts_of(&runner)
        .await
        .expect("the account read answers");
    assert_eq!(accounts, None);
    fixture.cleanup().await;
}

/// A session whose subject has no user row falls back to the account its
/// claim names, held as owner; a claim naming no account holds nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_fall_back_to_claim_when_session_subject_has_no_user_row() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let unknown = format!("user_unknown_{}", mint_id());

    let claimed = workspaces
        .accounts_of(&session(&fixture.john.tenant, &unknown))
        .await
        .expect("the account read answers")
        .expect("a person holds the claimed account");
    assert_eq!(claimed.home, id(&fixture.john.tenant));
    assert_eq!(claimed.held.len(), 1, "{claimed:?}");
    assert!(
        claimed
            .get(&fixture.john.tenant)
            .is_some_and(|account| account.role == Role::Owner)
    );

    let nowhere = mint_id();
    let empty = workspaces
        .accounts_of(&session(&nowhere, &unknown))
        .await
        .expect("the account read answers")
        .expect("a person always answers");
    assert_eq!(empty.home, id(&nowhere));
    assert!(empty.held.is_empty(), "{empty:?}");
    fixture.cleanup().await;
}

/// Bob walks every workspace of both accounts he holds one row at a time:
/// each appears once, in creation order, and a name both accounts use comes
/// back once per account.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_walk_keyset_across_held_accounts() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    for tenant in [&fixture.john.tenant, &fixture.bob.tenant] {
        let name = Chosen::parse(SHARED_NAME)
            .expect("a plain name")
            .expect("a chosen name");
        workspaces
            .create(&id(tenant), Some(name), "fixture", LATER)
            .await
            .expect("the shared name lands in each account");
    }
    let accounts = workspaces
        .accounts_of(&session(&fixture.bob.tenant, &fixture.bob.subject))
        .await
        .expect("the account read answers")
        .expect("Bob holds accounts");
    let tenants = accounts.tenants();
    let whole = workspaces
        .page(&tenants, None, None, 50)
        .await
        .expect("the whole list reads");

    let mut walked = Vec::new();
    let mut after: Option<After> = None;
    loop {
        let page = workspaces
            .page(&tenants, None, after.as_ref(), 1)
            .await
            .expect("a page reads");
        let Some(row) = page.rows.last() else { break };
        after = Some(After {
            created_at_ms: row.created_at_ms,
            id: id(&row.id),
        });
        walked.extend(page.rows.iter().map(|row| row.id.clone()));
        if !page.more {
            break;
        }
    }
    let listed: Vec<String> = whole.rows.iter().map(|row| row.id.clone()).collect();
    assert_eq!(
        walked, listed,
        "one row at a time is the whole list, in order"
    );
    assert!(
        whole.rows.len() >= 4,
        "two signups and two shared: {listed:?}"
    );

    let named = workspaces
        .page(&tenants, Some(SHARED_NAME), None, 50)
        .await
        .expect("the name filter reads");
    let mut owners: Vec<&str> = named
        .rows
        .iter()
        .map(|row| row.tenant_id.as_str())
        .collect();
    owners.sort_unstable();
    let mut expected = vec![fixture.john.tenant.as_str(), fixture.bob.tenant.as_str()];
    expected.sort_unstable();
    assert_eq!(owners, expected, "the shared name once per account");
    fixture.cleanup().await;
}

/// A chosen name the account already uses is refused as taken: the unique
/// index decides, and the refusal is the caller's, not the datastore's.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_refuse_chosen_workspace_name_when_account_already_uses_it() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let tenant = id(&fixture.john.tenant);
    let chosen = || {
        Chosen::parse(SHARED_NAME)
            .expect("a plain name")
            .expect("a chosen name")
    };
    workspaces
        .create(&tenant, Some(chosen()), "fixture", LATER)
        .await
        .expect("the first lands");
    let taken = workspaces
        .create(&tenant, Some(chosen()), "fixture", LATER)
        .await
        .expect_err("the name is taken");
    assert_eq!(taken.code(), error_code::WORKSPACE_NAME_EXISTS);
    fixture.cleanup().await;
}
