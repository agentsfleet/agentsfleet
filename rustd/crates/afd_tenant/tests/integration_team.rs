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
use afd_tenant::error::InviteConflict;
use afd_tenant::team::{Email, EmailStatus, INVITE_TTL_MS, Invitee, NewInvite, Removal, Team};
use afd_tenant::workspace::access::{ROLE_MEMBER, ROLE_OWNER};

use crate::access_lane::{Signup, delete_accounts, hold, id, sign_up};

/// Now, for every call a case makes.
const NOW: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// Long enough ago that an invite issued then has expired by [`NOW`].
const LONG_AGO: UnixMillis = UnixMillis::from_millis(1_700_000_000_000);

/// How many times the owner-removal race runs. Two concurrent removals that
/// lock in different orders deadlock in most rounds, so a dozen shows it.
const RACE_ROUNDS: usize = 12;

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
        let subject = format!("user_team_{}", mint_id());
        let signup = Signup {
            tenant: &person.tenant,
            user: &person.user,
            subject: &subject,
            email: &person.email,
            name: "team",
            display_name: None,
            workspace: &person.workspace,
        };
        sign_up(&self.database, &signup).await;
    }

    /// `person` as a member of John's account.
    async fn add_member(&self, person: &Person) {
        hold(&self.database, &self.john.tenant, &person.user, ROLE_MEMBER).await;
    }

    /// When `invite` was accepted and revoked, as stored.
    async fn stamps(&self, invite: &Uuid7) -> (Option<i64>, Option<i64>) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_as("SELECT accepted_at, revoked_at FROM core.invites WHERE id = $1::uuid")
            .bind(invite.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("the invite reads")
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

    /// The role `person` holds in John's account, if any.
    async fn role_in_johns(&self, person: &Person) -> Option<String> {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT role FROM core.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid",
        )
        .bind(&self.john.tenant)
        .bind(&person.user)
        .fetch_optional(&mut *connection)
        .await
        .expect("the role reads")
    }

    /// How many owners John's account has.
    async fn owners_in_johns(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let (owners,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM core.memberships WHERE tenant_id = $1::uuid AND role = $2",
        )
        .bind(&self.john.tenant)
        .bind(ROLE_OWNER)
        .fetch_one(&mut *connection)
        .await
        .expect("the owner count reads");
        owners
    }

    /// John invites `address`, as of `at`.
    async fn invite(&self, address: &str, at: UnixMillis) -> afd_tenant::Result<Uuid7> {
        let email = Email::parse(address, |_| true)?;
        let tenant = id(&self.john.tenant);
        let new = NewInvite {
            tenant: &tenant,
            inviter: &self.john.user_id,
            email: &email,
        };
        self.team.invite(&new, at).await.map(|invite| invite.id)
    }

    async fn cleanup(self) {
        let accounts = [&*self.john.tenant, &self.bob.tenant, &self.carol.tenant];
        delete_accounts(&self.database, accounts).await;
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
        .invitations(&tenant, NOW)
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
        .revoke_invitation(&tenant, &invite, NOW)
        .await
        .expect("the revoke lands");
    fixture
        .team
        .revoke_invitation(&tenant, &invite, NOW)
        .await
        .expect("a second revoke is quiet");
    assert!(
        fixture
            .team
            .invitations(&tenant, NOW)
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
        .revoke_invitation(&tenant, &invite, NOW)
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
    assert_eq!(twice.invite_conflict(), Some(InviteConflict::Invited));
    let member = fixture
        .invite(&fixture.bob.email, NOW)
        .await
        .expect_err("Bob is already a member");
    assert_eq!(member.code(), error_code::INVITE_CONFLICT);
    assert_eq!(member.invite_conflict(), Some(InviteConflict::Member));

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

/// An accept racing a revoke settles one way: Carol joins and the invite is
/// spent, or the revoke wins and she is told it is gone with no membership.
/// Never a membership from a revoked invite, never a refusal after a join.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_settle_one_outcome_when_accept_races_revoke() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    for round in 0..RACE_ROUNDS {
        let invite = fixture
            .invite(&fixture.carol.email, NOW)
            .await
            .expect("John invites Carol");
        let carol = fixture.carol.invitee();
        let (accepted, revoked) = tokio::join!(
            fixture.team.accept(&invite, &carol, NOW),
            fixture.team.revoke_invitation(&tenant, &invite, NOW),
        );
        revoked.expect("the revoke answers");
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
        match accepted {
            Ok(_) => assert_eq!(joined, 1, "round {round}: an accept that lands joins"),
            Err(refusal) => {
                assert_eq!(
                    refusal.code(),
                    error_code::INVITE_NOT_FOUND,
                    "round {round}"
                );
                assert_eq!(joined, 0, "round {round}: a refused accept joins nothing");
            }
        }
        if joined == 1 {
            fixture
                .team
                .remove(&tenant, &fixture.carol.user_id)
                .await
                .expect("Carol leaves for the next round");
        }
    }
    fixture.cleanup().await;
}

/// The instant an invite expires it is closed everywhere at once: the list,
/// the invitee's waiting list, a send, an accept, and a fresh invite to the
/// same address supersedes it. One millisecond earlier it is still open.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_treat_invite_as_closed_everywhere_when_now_equals_expiry() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    let issued = UnixMillis::from_millis(NOW.as_millis() - INVITE_TTL_MS);
    let just_before = UnixMillis::from_millis(NOW.as_millis() - 1);
    let invite = fixture
        .invite(&fixture.carol.email, issued)
        .await
        .expect("an invite that expires at NOW");

    let open = fixture
        .team
        .invitations(&tenant, just_before)
        .await
        .expect("lists");
    assert!(
        open.iter().any(|row| row.id == invite),
        "open a millisecond early"
    );

    let listed = fixture.team.invitations(&tenant, NOW).await.expect("lists");
    assert!(
        !listed.iter().any(|row| row.id == invite),
        "closed in the list"
    );
    let waiting = fixture
        .team
        .waiting_for(&fixture.carol.email, NOW)
        .await
        .expect("the waiting list reads");
    assert!(
        !waiting.iter().any(|row| row.id == invite.as_str()),
        "closed for Carol"
    );
    assert!(
        fixture
            .team
            .begin_email(&tenant, &invite, NOW)
            .await
            .expect("answers")
            .is_none(),
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
    assert!(accepted.workspaces.contains(&fixture.john.workspace));
    assert_eq!(
        fixture.role_in_johns(&fixture.carol).await.as_deref(),
        Some(ROLE_OWNER)
    );
    assert_eq!(fixture.memberships_in_johns(&fixture.carol).await, 1);
    let listed = fixture
        .team
        .invitations(&id(&fixture.john.tenant), NOW)
        .await
        .expect("lists");
    assert!(
        !listed.iter().any(|row| row.id == invite),
        "the invite is spent"
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
    let waiting = fixture
        .team
        .waiting_for(&shouted, NOW)
        .await
        .expect("reads");
    let found = waiting
        .iter()
        .find(|row| row.id == invite.as_str())
        .expect("found in any case");
    assert_eq!(found.tenant, fixture.john.tenant);

    fixture
        .team
        .revoke_invitation(&tenant, &invite, NOW)
        .await
        .expect("revokes");
    let waiting = fixture
        .team
        .waiting_for(&fixture.carol.email, NOW)
        .await
        .expect("reads");
    assert!(
        !waiting.iter().any(|row| row.id == invite.as_str()),
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
        .expect("accepted");
    let listed = fixture.team.invitations(&tenant, NOW).await.expect("lists");
    assert!(
        !listed.iter().any(|row| row.id == second),
        "accepted is gone"
    );
    fixture.cleanup().await;
}

/// The email record's edges: a send that began and never recorded reads as
/// failed; a later failed attempt keeps the earlier delivery's instant; an
/// accepted invite has nothing left to send.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_read_failed_when_attempt_began_and_never_recorded() {
    let fixture = Fixture::create().await;
    let tenant = id(&fixture.john.tenant);
    let later = UnixMillis::from_millis(NOW.as_millis() + 1);
    let invite = fixture
        .invite(&fixture.carol.email, NOW)
        .await
        .expect("John invites Carol");
    let status_of = |rows: &[afd_tenant::team::Invitation]| {
        let row = rows.iter().find(|row| row.id == invite).expect("listed");
        (row.email_status, row.email_sent_at_ms)
    };

    fixture
        .team
        .begin_email(&tenant, &invite, NOW)
        .await
        .expect("begins")
        .expect("pending");
    let listed = fixture.team.invitations(&tenant, NOW).await.expect("lists");
    assert_eq!(
        status_of(&listed),
        (EmailStatus::Failed, None),
        "begun, never recorded"
    );

    let second = fixture
        .team
        .begin_email(&tenant, &invite, NOW)
        .await
        .expect("begins")
        .expect("pending");
    fixture
        .team
        .record_email(&invite, second.attempt, EmailStatus::Sent, NOW)
        .await
        .expect("records");
    let third = fixture
        .team
        .begin_email(&tenant, &invite, later)
        .await
        .expect("begins")
        .expect("pending");
    fixture
        .team
        .record_email(&invite, third.attempt, EmailStatus::Failed, later)
        .await
        .expect("records");
    let listed = fixture
        .team
        .invitations(&tenant, later)
        .await
        .expect("lists");
    assert_eq!(
        status_of(&listed),
        (EmailStatus::Failed, Some(NOW.as_millis())),
        "the last delivery's instant survives a later failure"
    );

    fixture
        .team
        .accept(&invite, &fixture.carol.invitee(), later)
        .await
        .expect("accepted");
    assert!(
        fixture
            .team
            .begin_email(&tenant, &invite, later)
            .await
            .expect("answers")
            .is_none(),
        "an accepted invite has nothing to send"
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
    assert_ne!(refused.code(), error_code::INVITE_CONFLICT);
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
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
