//! Invites and members against live Postgres: issuing, accepting once,
//! refusing, conflicting, and the transaction that keeps a membership and its
//! accepted invite together.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::team::{Email, Invitee, NewInvite, Removal, Team};
use afd_tenant::workspace::access::{ROLE_MEMBER, ROLE_OWNER};

use crate::access_lane::id;

/// Now, for every call a case makes.
const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// Long enough ago that an invite issued then has expired by [`NOW`].
const LONG_AGO: UnixMillis = UnixMillis::from_millis(1_700_000_000_000);

/// One signed-up person.
struct Person {
    tenant: String,
    user: String,
    /// `user`, parsed once, for the invitee to borrow.
    user_id: Uuid7,
    email: String,
    workspace: String,
}

impl Person {
    fn minted(name: &str) -> Self {
        let user = mint_id();
        Self {
            tenant: mint_id(),
            user_id: id(&user),
            user,
            email: format!("{name}+{}@example.test", mint_id()),
            workspace: mint_id(),
        }
    }

    fn invitee(&self) -> Invitee<'_> {
        Invitee {
            user: &self.user_id,
            email: &self.email,
        }
    }
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    team: Team,
    john: Person,
    bob: Person,
    carol: Person,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let fixture = Self {
            team: Team::new(database.clone(), Entropy::new()),
            database,
            john: Person::minted("john"),
            bob: Person::minted("bob"),
            carol: Person::minted("carol"),
            lane,
        };
        for person in [&fixture.john, &fixture.bob, &fixture.carol] {
            fixture.sign_up(person).await;
        }
        fixture.add_member(&fixture.bob).await;
        fixture
    }

    async fn sign_up(&self, person: &Person) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'team', 1, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
               VALUES ($2::uuid, $1::uuid, $3, $4, NULL, 1, 1) \
             ), membership AS ( \
               INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
               VALUES ($5::uuid, $1::uuid, $2::uuid, $6, 1) \
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($7::uuid, $1::uuid, 'team', $3, 1)",
        )
        .bind(&person.tenant)
        .bind(&person.user)
        .bind(format!("user_team_{}", mint_id()))
        .bind(&person.email)
        .bind(mint_id())
        .bind(ROLE_OWNER)
        .bind(&person.workspace)
        .execute(&mut *connection)
        .await
        .expect("a signed-up person seeds");
    }

    /// `person` as a member of John's account.
    async fn add_member(&self, person: &Person) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 2)",
        )
        .bind(mint_id())
        .bind(&self.john.tenant)
        .bind(&person.user)
        .bind(ROLE_MEMBER)
        .execute(&mut *connection)
        .await
        .expect("a member seeds");
    }

    /// How many memberships `person` holds in John's account: zero or one.
    async fn memberships_in_johns(&self, person: &Person) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let (held,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM core.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid",
        )
        .bind(&self.john.tenant)
        .bind(&person.user)
        .fetch_one(&mut *connection)
        .await
        .expect("the membership count reads");
        held
    }

    /// John invites `address`, as of `at`.
    async fn invite(&self, address: &str, at: UnixMillis) -> afd_tenant::Result<Uuid7> {
        let email = Email::parse(address)?;
        let tenant = id(&self.john.tenant);
        let new = NewInvite {
            tenant: &tenant,
            inviter: &self.john.user_id,
            email: &email,
        };
        self.team.invite(&new, at).await.map(|invite| invite.id)
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id IN ($1::uuid, $2::uuid, $3::uuid)")
            .bind(&self.john.tenant)
            .bind(&self.bob.tenant)
            .bind(&self.carol.tenant)
            .execute(&mut *connection)
            .await
            .expect("the accounts clean up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

/// Dimension 3.1: an owner issues, lists and revokes; revoking twice is quiet.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_owner_manages_invites() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    let upper = fixture.carol.email.to_uppercase();

    let invite = fixture
        .invite(&upper, NOW)
        .await
        .expect("John invites Carol");
    let listed = fixture
        .team
        .invites(&tenant, NOW)
        .await
        .expect("the list reads");
    assert_eq!(listed.len(), 1);
    let first = listed.first().expect("one invite");
    assert_eq!(
        (first.id.clone(), first.email.as_str()),
        (invite.clone(), fixture.carol.email.as_str()),
        "stored lowercased"
    );

    fixture
        .team
        .revoke_invite(&tenant, &invite, NOW)
        .await
        .expect("the revoke lands");
    fixture
        .team
        .revoke_invite(&tenant, &invite, NOW)
        .await
        .expect("a second revoke is quiet");
    assert!(
        fixture
            .team
            .invites(&tenant, NOW)
            .await
            .expect("the list reads")
            .is_empty()
    );
    fixture.cleanup().await;
}

/// Dimension 3.2: accepting writes one membership, twice or concurrently.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_invitee_accepts_once() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let carol = fixture.carol.invitee();

    let (first, second) = tokio::join!(
        fixture.team.accept(&invite, &carol, NOW),
        fixture.team.accept(&invite, &carol, NOW),
    );
    let (first, second) = (
        first.expect("one accept lands"),
        second.expect("the other answers alike"),
    );
    assert_eq!(
        first, second,
        "a concurrent accept answers exactly as the first"
    );
    assert!(first.workspaces.contains(&fixture.john.workspace));
    let replay = fixture
        .team
        .accept(&invite, &carol, NOW)
        .await
        .expect("a replay answers");
    assert_eq!(replay, first);
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 1);

    let tenant = id(&fixture.john.tenant);
    let removed = fixture.team.remove(&tenant, &fixture.carol.user_id).await;
    assert_eq!(removed.expect("John removes Carol"), Removal::Removed);
    let rejoin = fixture
        .team
        .accept(&invite, &carol, NOW)
        .await
        .expect_err("an old link does not rejoin");
    assert_eq!(rejoin.code(), error_code::INVITE_NOT_FOUND);
    fixture.cleanup().await;
}

/// Dimension 3.3: another address is `UZ-INV-002`; expired and revoked are
/// `UZ-INV-001`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_accept_refusals() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");

    let elsewhere = fixture
        .team
        .accept(&invite, &fixture.bob.invitee(), NOW)
        .await
        .expect_err("Bob is not Carol");
    assert_eq!(elsewhere.code(), error_code::INVITE_EMAIL_MISMATCH);

    fixture
        .team
        .revoke_invite(&tenant, &invite, NOW)
        .await
        .expect("the revoke lands");
    let revoked = fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await
        .expect_err("revoked");
    assert_eq!(revoked.code(), error_code::INVITE_NOT_FOUND);

    let stale = fixture
        .invite(&fixture.carol.email, LONG_AGO)
        .await
        .expect("an old invite issues");
    let expired = fixture
        .team
        .accept(&stale, &fixture.carol.invitee(), NOW)
        .await
        .expect_err("expired");
    assert_eq!(expired.code(), error_code::INVITE_NOT_FOUND);
    let unknown = fixture
        .team
        .accept(&id(&mint_id()), &fixture.carol.invitee(), NOW)
        .await
        .expect_err("never issued");
    assert_eq!(unknown.code(), error_code::INVITE_NOT_FOUND);
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 0);
    fixture.cleanup().await;
}

/// Dimension 3.4: a second pending invite and an invite for a member are
/// `UZ-INV-003`; an expired one is superseded; the last owner stays.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_invite_and_member_conflicts() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);

    fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("the first invite issues");
    let twice = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect_err("one pending invite per address");
    assert_eq!(twice.code(), error_code::INVITE_CONFLICT);
    let member = fixture
        .invite(&fixture.bob.email, NOW)
        .await
        .expect_err("Bob is already a member");
    assert_eq!(member.code(), error_code::INVITE_CONFLICT);

    let stranger = format!("dave+{}@example.test", mint_id());
    fixture
        .invite(&stranger, LONG_AGO)
        .await
        .expect("an old invite issues");
    fixture
        .invite(&stranger, NOW)
        .await
        .expect("an expired invite is superseded, not a conflict");

    let last = fixture
        .team
        .remove(&tenant, &fixture.john.user_id)
        .await
        .expect_err("the last owner stays");
    assert_eq!(last.code(), error_code::MEMBER_LAST_OWNER);
    let bob = &fixture.bob.user_id;
    assert_eq!(
        fixture
            .team
            .remove(&tenant, bob)
            .await
            .expect("Bob is removed"),
        Removal::Removed
    );
    assert_eq!(
        fixture
            .team
            .remove(&tenant, bob)
            .await
            .expect("removing again is quiet"),
        Removal::Absent
    );
    fixture.cleanup().await;
}

/// Invariant 3: the membership and the accepted invite commit together. The
/// invite's stamp is made to fail after the membership is written, and no
/// membership survives the failure.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_accept_whose_second_write_fails_leaves_no_membership() {
    let fixture = Fixture::create().await;
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let migrator = fixture.lane.open(DbRole::Migrator, &[]).await;
    let name = format!("fixture_refuse_accept_{}", mint_id().replace('-', ""));
    let mut connection = migrator.acquire().await.expect("a migrator connection");
    // `AssertSqlSafe`: DDL takes no bind parameter, and every interpolated
    // value is this fixture's own minted identifier, never a caller's.
    for ddl in [
        format!(
            "CREATE FUNCTION {name}() RETURNS trigger AS $$ BEGIN \
               RAISE EXCEPTION 'fixture: the invite refuses its stamp'; \
             END $$ LANGUAGE plpgsql"
        ),
        format!(
            "CREATE TRIGGER {name} BEFORE UPDATE ON core.invites FOR EACH ROW \
             WHEN (OLD.id = '{}'::uuid) EXECUTE FUNCTION {name}()",
            invite.as_str()
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(ddl))
            .execute(&mut *connection)
            .await
            .expect("the refusal installs");
    }

    let refused = fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), NOW)
        .await;

    for ddl in [
        format!("DROP TRIGGER IF EXISTS {name} ON core.invites"),
        format!("DROP FUNCTION IF EXISTS {name}()"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(ddl))
            .execute(&mut *connection)
            .await
            .expect("the refusal lifts");
    }
    drop(connection);
    assert!(refused.is_err(), "the failed stamp fails the accept");
    assert_eq!(
        fixture.memberships_in_johns(&fixture.carol).await,
        0,
        "the membership rolled back with it"
    );
    fixture.cleanup().await;
}
