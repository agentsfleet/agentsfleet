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

use super::harness;
use super::vendor::Vendor;

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
pub(super) struct Fixture {
    lane: TestDatabase,
    pub(super) database: Db,
    pub(super) queue: Redis,
    pub(super) subject: String,
    tenant: String,
    pub(super) workspace: Uuid7,
    admin: Uuid7,
    user: String,
    key: String,
    pub(super) token: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            queue: harness::connect_redis().await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            workspace: minted(),
            admin: minted(),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{bits}"),
            lane,
        }
    }

    /// The production router over live stores, with `vendor` standing in for
    /// the provider's token endpoint.
    pub(super) fn router(&self, vendor: &Vendor) -> axum::Router {
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_owned_workspace(self.workspace.clone())
        .with_platform_admin(self.admin.clone())
        .with_live_connectors(self.database.clone(), self.queue.clone(), vendor.url())
        .router()
    }

    pub(super) async fn seed(&self) {
        self.seed_rows().await;
        self.seal(STATE_KEY, &format!(r#"{{"{SECRET_FIELD}":"{STATE_SECRET}"}}"#))
            .await;
        self.seal(
            &Provider::Slack.app_key(),
            &format!(r#"{{"client_id":"{CLIENT_ID}","client_secret":"{CLIENT_SECRET}"}}"#),
        )
        .await;
    }

    /// The tenant, the connected workspace, the admin workspace, and the owner.
    async fn seed_rows(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Connector callback live', 1, 1) \
             ), workspaces AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'connected', $3, 1), \
                      ($4::uuid, $1::uuid, 'platform-admin', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($5::uuid, $1::uuid, $3, 'connector-live@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($6::uuid, $1::uuid, 'fixture', '', $7, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(self.admin.as_str())
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the tenant, its workspaces and its owner seed");
    }

    /// Seals `document` into the admin workspace under `key`.
    ///
    /// Through the real vault under the harness's own key rather than an INSERT
    /// of ciphertext: a row this fixture hand-wrote would be one the route could
    /// not open, and every reader here answers "not configured" for that — a
    /// refusal indistinguishable from having stored nothing at all.
    async fn seal(&self, key: &str, document: &str) {
        let raw = serde_json::value::RawValue::from_string(document.to_owned())
            .expect("the fixture credential is an object");
        let sealed = harness::vault(self.database.clone())
            .create(
                &self.admin,
                &SecretName::parse(key).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture credential is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await;
        // Named in the message rather than left to `expect`: this seals two
        // secrets under different keys, and a failure that did not say which
        // reads as the route being unconfigured for both.
        assert!(sealed.is_ok(), "the fixture secret {key} seals: {sealed:?}");
    }

    /// The grant this workspace holds for `provider`, opened.
    ///
    /// Read through the vault rather than off the row, because the row holds
    /// ciphertext: what the assertion is about is the HANDLE a runner will open
    /// when a fleet declares this integration.
    pub(super) async fn grant(&self, provider: Provider) -> Option<serde_json::Value> {
        let name = SecretName::parse(provider.grant_key()).expect("a provider key is storable");
        let opened = harness::vault(self.database.clone())
            .load(&self.workspace, &name)
            .await
            .expect("the vault answers");
        opened.map(|bytes| {
            serde_json::from_slice(bytes.expose()).expect("a sealed grant is a JSON object")
        })
    }

    /// How many secrets this workspace holds, by name.
    ///
    /// A count as well as a read: a second connect that sealed under a second
    /// name would leave the first grant intact and pass a read-only assertion
    /// while a runner opened the wrong one.
    pub(super) async fn secret_names(&self) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "SELECT key_name FROM vault.secrets WHERE workspace_id = $1::uuid ORDER BY key_name",
        )
            .bind(self.workspace.as_str())
            .fetch_all(&mut *connection)
            .await
            .expect("the workspace's secret names read")
            .iter()
            .map(|row| row.get("key_name"))
            .collect()
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        for workspace in [&self.workspace, &self.admin] {
            sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid")
                .bind(workspace.as_str())
                .execute(&mut *transaction)
                .await
                .expect("the fixture's sealed secrets clean up");
        }
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the scoped fixture cleans up");
        transaction
            .commit()
            .await
            .expect("the scoped cleanup commits");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

/// A minted workspace identifier, parsed once.
fn minted() -> Uuid7 {
    Uuid7::parse(&mint_id()).expect("a minted workspace is canonical")
}
