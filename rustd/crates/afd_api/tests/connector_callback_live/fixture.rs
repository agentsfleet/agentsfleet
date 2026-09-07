//! The rows and the sealed secrets a completed connect needs.
//!
//! Three things have to exist before a callback can land a grant, and the route
//! answers the SAME refusal for each of them missing: this deployment's app
//! credentials for the provider, the secret its connect states are signed with,
//! and a workspace the person presenting the callback holds.
//!
//! # The signing secret is the approval one, and that is not a copy
//!
//! `connector::state_secret` reads `APPROVAL_IDENTITY` — one deployment secret
//! serves both the approval callbacks and the connect states, which is the
//! Zig's `approval_signing_secret` doing the same. A fixture sealing two would
//! be inventing a split the daemon does not have.

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_redis::Redis;
use afd_vault::{SecretBody, SecretName};
use sqlx::Row as _;

use super::fake_provider::FakeProvider;
use super::harness;

/// The vault key this deployment's connect-state signing secret is held under.
const STATE_KEY: &str = afd_http::services::APPROVAL_IDENTITY;

/// The field a stored webhook credential carries its secret in.
const SECRET_FIELD: &str = "webhook_secret";

/// What this deployment's connect states are signed with.
const STATE_SECRET: &str = "fixture-connect-state-secret";

/// The client identity this deployment registered with the provider.
const CLIENT_ID: &str = "fixture-client-id";

/// Its secret half.
const CLIENT_SECRET: &str = "fixture-client-secret";

/// What every fixture person's subject starts with.
///
/// Minted per fixture rather than named once for the file: the column is unique
/// deployment-wide and these tests seed in parallel, so a shared spelling makes
/// the first fixture to commit win and the rest fail their seed.
const SUBJECT_PREFIX: &str = "user_live_connector_callback_";

/// A workspace, its owner, and a deployment configured to connect Slack.
pub(crate) struct Fixture {
    lane: TestDatabase,
    pub(crate) database: Db,
    pub(crate) queue: Redis,
    pub(crate) subject: String,
    /// The provider account this fixture's grant is scoped to.
    ///
    /// Minted per fixture for the same reason [`SUBJECT_PREFIX`] is:
    /// `core.connector_installs` is unique on `(provider, external_account_id)`
    /// deployment-wide, and these tests run in parallel against one database.
    /// A team id named once for the file makes every fixture contend for a
    /// single row — the first to land holds it, and a sibling asserting that
    /// ITS connect left no row reads the neighbour's instead of its own.
    pub(crate) team: String,
    /// A second authenticated person, who did not start the connect.
    pub(crate) bystander: String,
    tenant: String,
    pub(crate) workspace: Uuid7,
    admin: Uuid7,
    user: String,
    key: String,
    pub(crate) token: String,
    /// The bystander's own row and credential — see [`Self::bystander`].
    bystander_user: String,
    bystander_key: String,
    pub(crate) bystander_token: String,
}

impl Fixture {
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            queue: harness::connect_redis().await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            team: format!("T0FIX{}", mint_id().replace('-', "").to_uppercase()),
            bystander: format!("{SUBJECT_PREFIX}bystander_{}", mint_id()),
            tenant: mint_id(),
            workspace: minted(),
            admin: minted(),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{bits}"),
            bystander_user: mint_id(),
            bystander_key: mint_id(),
            bystander_token: format!(
                "agt_t{}{}",
                mint_id().replace('-', ""),
                mint_id().replace('-', "")
            ),
            lane,
        }
    }

    /// The production router over live stores, with `provider` standing in
    /// for the real one's token endpoint.
    pub(crate) fn router(&self, provider: &FakeProvider) -> axum::Router {
        self.router_as(provider, &self.subject)
    }

    /// The same daemon, with `subject` as the person holding the bearer.
    ///
    /// The bystander case: another authenticated person, who even OWNS the
    /// workspace, presenting the starter's callback. Everything else about
    /// the daemon is identical, so the one refusal that fires is the state's.
    pub(crate) fn router_as(&self, provider: &FakeProvider, subject: &str) -> axum::Router {
        harness::Fleet::live(
            self.database.clone(),
            subject,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_owned_workspace(self.workspace.clone())
        .with_platform_admin(self.admin.clone())
        .with_live_connectors(self.database.clone(), self.queue.clone(), provider.url())
        .router()
    }

    /// The workspaces `core.connector_installs` routes `account` to, for
    /// `provider` — empty when nothing routes it.
    pub(crate) async fn routed_to(&self, provider: Provider, account: &str) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "SELECT workspace_id::text FROM core.connector_installs \
             WHERE provider = $1 AND external_account_id = $2 ORDER BY workspace_id",
        )
        .bind(provider.id())
        .bind(account)
        .fetch_all(&mut *connection)
        .await
        .expect("the routing rows read")
        .iter()
        .map(|row| row.get("workspace_id"))
        .collect()
    }

    /// The admin workspace's id — the "other workspace" of the exclusive
    /// claim's refusal.
    pub(crate) fn admin_workspace(&self) -> &str {
        self.admin.as_str()
    }

    /// Routes `account` to the ADMIN workspace — some other workspace, from the
    /// tenant workspace's point of view — as an earlier connect would have.
    pub(crate) async fn route_elsewhere(&self, provider: Provider, account: &str) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.connector_installs \
               (id, provider, external_account_id, workspace_id, installed_by, \
                scopes, created_at, updated_at) \
             VALUES ($1::uuid, $2, $3, $4::uuid, '', ARRAY[]::TEXT[], 1, 1)",
        )
        .bind(mint_id())
        .bind(provider.id())
        .bind(account)
        .bind(self.admin.as_str())
        .execute(&mut *connection)
        .await
        .expect("the foreign routing row seeds");
    }

    /// Makes the vault refuse every seal for THIS workspace, as a datastore
    /// that fails mid-transaction would.
    ///
    /// A `BEFORE INSERT` trigger on `vault.secrets`, scoped to the fixture's
    /// workspace so a sibling test's seals are untouched, installed under the
    /// migrator role that owns the schema. Failure injection at the second
    /// write of the landing transaction, which is the one place a stub cannot
    /// reach: the property under test is that the FIRST write unwinds with it.
    pub(crate) async fn refuse_seals(&self) -> RefusedSeals {
        let migrator = self.lane.open(DbRole::Migrator, &[]).await;
        let name = format!("fixture_refuse_seal_{}", mint_id().replace('-', ""));
        let mut connection = migrator.acquire().await.expect("a migrator connection");
        // `AssertSqlSafe`: DDL takes no bind parameter, and every interpolated
        // value is this fixture's own — a minted identifier and a workspace id
        // it minted — never a caller's.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE FUNCTION {name}() RETURNS trigger AS $$ BEGIN \
               IF NEW.workspace_id = '{}'::uuid THEN \
                 RAISE EXCEPTION 'fixture: the vault refuses this seal'; \
               END IF; \
               RETURN NEW; \
             END $$ LANGUAGE plpgsql",
            self.workspace.as_str()
        )))
        .execute(&mut *connection)
        .await
        .expect("the refusing function installs");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE TRIGGER {name} BEFORE INSERT ON vault.secrets \
             FOR EACH ROW EXECUTE FUNCTION {name}()"
        )))
        .execute(&mut *connection)
        .await
        .expect("the refusing trigger installs");
        drop(connection);
        RefusedSeals { migrator, name }
    }
}

/// A vault refusal in force — see [`Fixture::refuse_seals`]. Lifted by
/// [`Self::lift`], which a test calls before its cleanup.
pub(crate) struct RefusedSeals {
    migrator: Db,
    name: String,
}

impl RefusedSeals {
    /// Removes the trigger and its function.
    pub(crate) async fn lift(self) {
        let mut connection = self
            .migrator
            .acquire()
            .await
            .expect("a migrator connection");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER IF EXISTS {} ON vault.secrets",
            self.name
        )))
        .execute(&mut *connection)
        .await
        .expect("the refusing trigger lifts");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP FUNCTION IF EXISTS {}()",
            self.name
        )))
        .execute(&mut *connection)
        .await
        .expect("the refusing function lifts");
    }
}

/// A minted workspace identifier, parsed once.
fn minted() -> Uuid7 {
    Uuid7::parse(&mint_id()).expect("a minted workspace is canonical")
}

#[path = "fixture/seeding.rs"]
mod seeding;
