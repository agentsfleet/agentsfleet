//! Three people over live Postgres, for the suites whose subject is the access
//! decision itself.
//!
//! John owns an account with a workspace and a fleet. Bob owns his own account
//! and is a member of John's. A stranger owns an account and holds nothing of
//! John's. Each signs in with a browser session, the one credential resolved
//! through the user row a membership hangs from.

use afd_auth::scope::{Scope, ScopeSet, TENANT_OWNER_GRANT};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_dragonfly::{Dragonfly, SubscriptionHub};
use afd_tenant::test_util::{Signup, delete_accounts, sign_up};
use afd_tenant::workspace::access::ROLE_MEMBER;
use axum::Router;

use crate::harness::Fleet;

/// What a signed-up owner holds: the grant signup writes to the provider.
pub(crate) fn owner_scopes() -> ScopeSet {
    ScopeSet::from_scopes(&TENANT_OWNER_GRANT)
}

/// What a platform operator holds: their own account's grant, plus the scope
/// that crosses into every other account's workspaces.
pub(crate) const fn platform_scopes() -> ScopeSet {
    ScopeSet::from_scopes(&TENANT_OWNER_GRANT).insert(Scope::WorkspaceAny)
}

/// One signed-up person and the account they own.
pub(crate) struct Person {
    pub(crate) subject: String,
    pub(crate) display_name: &'static str,
    /// Unique per run: the invites waiting for an address are read across
    /// every account, so a shared address would see another run's.
    pub(crate) email: String,
    pub(crate) tenant: String,
    pub(crate) user: String,
    pub(crate) workspace: Uuid7,
    pub(crate) token: String,
}

impl Person {
    fn minted(display_name: &'static str) -> Self {
        let subject = format!("user_members_{}", mint_id());
        Self {
            token: format!("session-{subject}"),
            email: format!("{subject}@example.test"),
            subject,
            display_name,
            tenant: mint_id(),
            user: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
        }
    }
}

/// John's account, Bob's membership in it, and a stranger.
pub(crate) struct Members {
    lane: TestDatabase,
    pub(crate) database: Db,
    pub(crate) john: Person,
    pub(crate) bob: Person,
    pub(crate) stranger: Person,
    /// John's fleet, the one a member reads and steers.
    pub(crate) fleet: Uuid7,
}

impl Members {
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            john: Person::minted("John"),
            bob: Person::minted("Bob"),
            stranger: Person::minted("Stranger"),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            lane,
        }
    }

    /// The three accounts, John's fleet, and Bob's membership in John's account.
    pub(crate) async fn seed(&self) {
        for person in [&self.john, &self.bob, &self.stranger] {
            self.sign_up(person).await;
        }
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH fleet AS ( \
               INSERT INTO core.fleets \
                 (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                  status, created_at, updated_at) \
               VALUES ($1::uuid, $2::uuid, $3::uuid, 'members', '# fixture', '{}', \
                       'active', 1, 1) \
             ) \
             INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
             VALUES ($4::uuid, $3::uuid, $5::uuid, $6, 2)",
        )
        .bind(self.fleet.as_str())
        .bind(self.john.workspace.as_str())
        .bind(&self.john.tenant)
        .bind(mint_id())
        .bind(&self.bob.user)
        .bind(ROLE_MEMBER)
        .execute(&mut *connection)
        .await
        .expect("John's fleet and Bob's membership seed");
    }

    /// The rows signup writes for one person, less the wallet.
    async fn sign_up(&self, person: &Person) {
        let name = person.display_name.to_lowercase();
        let signup = Signup {
            tenant: &person.tenant,
            user: &person.user,
            subject: &person.subject,
            email: &person.email,
            name: &name,
            display_name: Some(person.display_name),
            workspace: person.workspace.as_str(),
        };
        sign_up(&self.database, &signup).await;
    }

    /// John removes Bob from his account.
    pub(crate) async fn remove_bob(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(afd_tenant::sql::member::DELETE_MEMBERSHIP)
            .bind(&self.john.tenant)
            .bind(&self.bob.user)
            .execute(&mut *connection)
            .await
            .expect("Bob's membership is removed");
    }

    /// The actor of every message admitted to John's fleet.
    pub(crate) async fn admitted_actors(&self) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar("SELECT actor FROM core.fleet_admissions WHERE fleet_id = $1::uuid")
            .bind(self.fleet.as_str())
            .fetch_all(&mut *connection)
            .await
            .expect("John's fleet's admissions read")
    }

    /// A router signed in as `who`, deciding access from these rows.
    pub(crate) fn router(&self, who: &Person, scopes: ScopeSet) -> Router {
        Fleet::live(self.database.clone(), &who.subject, scopes)
            .with_live_ownership()
            .with_dashboard_holding(&who.subject, scopes)
            .router()
    }

    /// The same, able to steer and stream through the lane's Dragonfly.
    pub(crate) fn live_router(
        &self,
        who: &Person,
        scopes: ScopeSet,
        queue: Dragonfly,
        hub: SubscriptionHub,
    ) -> Router {
        Fleet::live(self.database.clone(), &who.subject, scopes)
            .with_live_ownership()
            .with_dashboard_holding(&who.subject, scopes)
            .with_steering_queue(self.database.clone(), queue)
            .with_live_hub(hub)
            .router()
    }

    pub(crate) async fn cleanup(self) {
        let accounts = [&*self.john.tenant, &self.bob.tenant, &self.stranger.tenant];
        delete_accounts(&self.database, &accounts).await;
        drop(self.database);
        self.lane.cleanup().await;
    }
}
