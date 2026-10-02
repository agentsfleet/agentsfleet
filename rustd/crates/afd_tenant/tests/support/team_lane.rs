//! What the team suites ask of the shared fixture: John's invite, and the rows
//! an invite or a removal leaves behind, read back from Postgres rather than
//! taken from the store's own answer.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_tenant::team::{Email, EmailAttempt, Invitation, NewInvite, Waiting};
use afd_tenant::workspace::access::ROLE_OWNER;

use crate::access_lane::{Account, Fixture, id};

/// What a readback reports when the lane gives it no connection.
const NO_CONNECTION: &str = "an API connection";

/// Where a UUID spells its variant, as `afd_core::id` reads it.
const VARIANT_AT: usize = 19;

impl Fixture {
    /// John invites `address`, as of `at`.
    pub(crate) async fn invite(&self, address: &str, at: UnixMillis) -> afd_tenant::Result<Uuid7> {
        let email = Email::parse(address, |_| true)?;
        let tenant = id(&self.john.tenant);
        let new = NewInvite {
            tenant: &tenant,
            inviter: &self.john.user_id,
            email: &email,
        };
        self.team.invite(&new, at).await.map(|invite| invite.id)
    }

    /// John's invites that can still be accepted at `at`.
    pub(crate) async fn johns_invites(&self, at: UnixMillis) -> Vec<Invitation> {
        let tenant = id(&self.john.tenant);
        let listed = self.team.invitations(&tenant, at).await;
        listed.expect("John's invite list reads")
    }

    /// Whether `invite` is among John's open invites at `at`.
    pub(crate) async fn johns_lists(&self, invite: &Uuid7, at: UnixMillis) -> bool {
        let listed = self.johns_invites(at).await;
        listed.iter().any(|row| row.id == *invite)
    }

    /// The invites waiting for `address` at `at`.
    pub(crate) async fn waiting_for(&self, address: &str, at: UnixMillis) -> Vec<Waiting> {
        let waiting = self.team.waiting_for(address, at).await;
        waiting.expect("the waiting list reads")
    }

    /// One more send of John's `invite`, counted at `at`; `None` when the
    /// invite has nothing left to send.
    pub(crate) async fn begin_email(&self, invite: &Uuid7, at: UnixMillis) -> Option<EmailAttempt> {
        let tenant = id(&self.john.tenant);
        let begun = self.team.begin_email(&tenant, invite, at).await;
        begun.expect("the send answers")
    }

    /// When `invite` was accepted and revoked, as stored.
    pub(crate) async fn stamps(&self, invite: &Uuid7) -> (Option<i64>, Option<i64>) {
        let mut connection = self.database.acquire().await.expect(NO_CONNECTION);
        sqlx::query_as("SELECT accepted_at, revoked_at FROM core.invites WHERE id = $1::uuid")
            .bind(invite.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("the invite's stamps read")
    }

    /// How many memberships `person` holds in John's account: zero or one.
    pub(crate) async fn memberships_in_johns(&self, person: &Account) -> i64 {
        let mut connection = self.database.acquire().await.expect(NO_CONNECTION);
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
    pub(crate) async fn role_in_johns(&self, person: &Account) -> Option<String> {
        let mut connection = self.database.acquire().await.expect(NO_CONNECTION);
        sqlx::query_scalar(
            "SELECT role FROM core.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid",
        )
        .bind(&self.john.tenant)
        .bind(&person.user)
        .fetch_optional(&mut *connection)
        .await
        .expect("the role reads")
    }

    /// Adds a workspace to John's account whose identifier Postgres takes and
    /// this daemon cannot read: a version-7 UUID with no RFC 4122 variant.
    pub(crate) async fn add_unreadable_workspace(&self) -> String {
        let mut unreadable = mint_id();
        unreadable.replace_range(VARIANT_AT..=VARIANT_AT, "0");
        let mut connection = self.database.acquire().await.expect(NO_CONNECTION);
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($1::uuid, $2::uuid, NULL, NULL, 2)",
        )
        .bind(&unreadable)
        .bind(&self.john.tenant)
        .execute(&mut *connection)
        .await
        .expect("Postgres takes the unreadable workspace");
        unreadable
    }

    /// Removes the workspace `id`.
    pub(crate) async fn remove_workspace(&self, id: &str) {
        let mut connection = self.database.acquire().await.expect(NO_CONNECTION);
        sqlx::query("DELETE FROM core.workspaces WHERE id = $1::uuid")
            .bind(id)
            .execute(&mut *connection)
            .await
            .expect("the workspace is removed");
    }

    /// How many owners John's account has.
    pub(crate) async fn owners_in_johns(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect(NO_CONNECTION);
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
}
