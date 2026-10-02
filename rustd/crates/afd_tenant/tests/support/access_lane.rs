//! The principals, verdicts and people the access suites build, spelled once.
//!
//! Every access suite asks the same resolver about the same kinds of caller:
//! a browser session naming a subject, and a claim-bound credential naming a
//! tenant. These are the constructors for both and for the verdicts they get,
//! and the three people the team and membership suites seed: John owns an
//! account, Bob owns his own and is a member of John's, and Carol owns hers
//! and holds nothing of John's. The rows themselves are
//! `afd_tenant::test_util`'s, which `afd_api`'s suites seed too.

use afd_auth::principal::{Person, PersonCredential, Principal, Subject};
use afd_auth::scope::ScopeSet;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::team::{Invitee, Team};
use afd_tenant::test_util::{Signup, delete_accounts, hold, sign_up};
use afd_tenant::workspace::access::{Access, Grant, ROLE_MEMBER, Role};

/// A stored or minted identifier, parsed.
pub(crate) fn id(value: &str) -> Uuid7 {
    Uuid7::parse(value).expect("the fixture identifier is UUIDv7")
}

/// A person proven by `credential`, claiming `tenant` as `subject`.
pub(crate) fn person(
    credential: PersonCredential,
    tenant: &str,
    subject: &str,
    scopes: ScopeSet,
) -> Principal {
    Principal::Person(Person::new(
        credential,
        id(tenant),
        Subject::new(subject).expect("the fixture subject is not blank"),
        scopes,
    ))
}

/// A browser session for `subject`, whose token claims `tenant`.
pub(crate) fn session(tenant: &str, subject: &str) -> Principal {
    person(
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        tenant,
        subject,
        ScopeSet::EMPTY,
    )
}

/// The verdict for `tenant`'s workspace held with `role`.
pub(crate) fn held(tenant: &str, role: Role) -> Access {
    Access {
        tenant: id(tenant),
        grant: Grant::Membership(role),
    }
}

/// One signed-up person: their account, user row, subject, address and the
/// account's one workspace.
pub(crate) struct Account {
    /// What they are called, and what their account is named after.
    pub(crate) name: &'static str,
    pub(crate) tenant: String,
    pub(crate) user: String,
    /// `user`, parsed once, for an invitee to borrow.
    pub(crate) user_id: Uuid7,
    pub(crate) subject: String,
    /// Unique per run: the invites waiting for an address are read across
    /// every account, so a shared address would see another run's.
    pub(crate) email: String,
    pub(crate) workspace: String,
}

impl Account {
    /// A person called `name`, every identifier minted.
    pub(crate) fn minted(name: &'static str) -> Self {
        let user = mint_id();
        let handle = name.to_lowercase();
        Self {
            name,
            tenant: mint_id(),
            user_id: id(&user),
            user,
            subject: format!("user_{handle}_{}", mint_id()),
            email: format!("{handle}+{}@example.test", mint_id()),
            workspace: mint_id(),
        }
    }

    /// Writes this person's account as signup leaves it, named after them.
    pub(crate) async fn sign_up(&self, database: &Db) {
        let signup = Signup {
            tenant: &self.tenant,
            user: &self.user,
            subject: &self.subject,
            email: &self.email,
            name: self.name,
            display_name: Some(self.name),
            workspace: &self.workspace,
        };
        sign_up(database, &signup).await;
    }

    /// This person, accepting an invite.
    pub(crate) fn invitee(&self) -> Invitee<'_> {
        Invitee {
            user: &self.user_id,
            email: &self.email,
        }
    }
}

/// John, Bob and Carol over the lane's database, and the team store.
pub(crate) struct Fixture {
    pub(crate) lane: TestDatabase,
    pub(crate) database: Db,
    pub(crate) team: Team,
    pub(crate) john: Account,
    pub(crate) bob: Account,
    pub(crate) carol: Account,
}

impl Fixture {
    /// The three accounts, and Bob's membership in John's.
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let fixture = Self {
            team: Team::new(database.clone(), Entropy::new()),
            database,
            john: Account::minted("John"),
            bob: Account::minted("Bob"),
            carol: Account::minted("Carol"),
            lane,
        };
        for account in [&fixture.john, &fixture.bob, &fixture.carol] {
            account.sign_up(&fixture.database).await;
        }
        fixture.set_bob_in_johns_account(ROLE_MEMBER).await;
        fixture
    }

    /// Bob's row in John's account, holding `role` whatever it held before.
    pub(crate) async fn set_bob_in_johns_account(&self, role: &str) {
        hold(&self.database, &self.john.tenant, &self.bob.user, role).await;
    }

    /// Removes the three accounts and releases the lane.
    pub(crate) async fn cleanup(self) {
        let accounts = [&*self.john.tenant, &self.bob.tenant, &self.carol.tenant];
        delete_accounts(&self.database, &accounts).await;
        drop(self.team);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
