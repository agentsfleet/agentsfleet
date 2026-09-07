//! What a workspace holds, read and let go over the vault that actually holds it.
//!
//! `afd_connector::grant::holding` is where a connection is read, listed and
//! forgotten, and all four of its readers ran zero covered lines: every suite in
//! front of them either stops at a refusal before the vault or injects a stub.
//! The four are only honest against a real one — each is a `load` or a `list`
//! whose whole job is deciding what a STORED envelope means, and a stub deciding
//! that instead is a test grading its own fixture.
//!
//! # The marker is the load-bearing part
//!
//! A workspace vault holds ordinary secrets beside connector handles, and the
//! grant key is just the provider id. So "is this workspace connected to Slack"
//! cannot be answered by the key alone — a person who stored their own secret
//! named `slack` would read as connected, and the dashboard would offer a
//! disconnect for something that was never a connection. The `integration`
//! marker is what separates them, and the case is here rather than in a unit
//! test because it is the stored bytes that have to be wrong.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_vault::{SecretBody, SecretName};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// What this suite's person is named before its own identifier is appended.
///
/// A prefix rather than a constant subject: `core.users.oidc_subject` is unique
/// deployment-wide, so a fixture spelling it once would collide with its own
/// leftovers the moment a run failed before cleanup.
const SUBJECT_PREFIX: &str = "user_live_connector_status_";

/// The provider the fixture workspace holds a handle for.
const HELD: Provider = Provider::Slack;

/// One it holds nothing for.
const UNHELD: Provider = Provider::Jira;

/// What the stored handle calls itself.
const LABEL: &str = "Acme Workspace";

/// The wire spellings `afd_wire::connector` declares.
const STATUS_CONNECTED: &str = "connected";
/// See [`STATUS_CONNECTED`].
const STATUS_NOT_CONNECTED: &str = "not_connected";

/// The provider account the held connection routes inbound events from.
const ROUTED_ACCOUNT: &str = "T0STATUSFIXTURE";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_reads_lists_and_lets_go_of_what_its_vault_actually_holds() {
    // One test rather than six, because every step past the first depends on
    // the state the one before it left: a disconnect only means something
    // against a connection that was there, and the second disconnect only means
    // something after the first removed it. Split, each would be asserting
    // against state it had set up for itself.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        &fixture.subject,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    fixture.seal_handle(HELD, &handle(LABEL)).await;
    fixture.route(HELD, ROUTED_ACCOUNT).await;

    a_held_handle_reads_as_connected(&router, &fixture).await;
    a_provider_with_no_handle_reads_as_not_connected(&router, &fixture).await;
    the_catalogue_marks_only_what_is_held(&router, &fixture).await;
    a_disconnect_removes_the_handle_and_repeats_harmlessly(&router, &fixture).await;
    a_secret_that_is_not_a_connector_handle_is_not_a_connection(&router, &fixture).await;

    fixture.cleanup().await;
}

/// A tenant, its workspace, the person acting, and their key.
struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    subject: String,
    user: String,
    key: String,
    token: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let first = mint_id().replace('-', "");
        let second = mint_id().replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{first}{second}"),
            lane,
        }
    }

    /// Routes `account` to this workspace, as a landed connect would have.
    async fn route(&self, provider: Provider, account: &str) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.connector_installs \
               (id, provider, external_account_id, workspace_id, installed_by, \
                scopes, created_at, updated_at) \
             VALUES ($1::uuid, $2, $3, $4::uuid, $5, ARRAY[]::TEXT[], 1, 1)",
        )
        .bind(mint_id())
        .bind(provider.id())
        .bind(account)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .execute(&mut *connection)
        .await
        .expect("the routing row seeds");
    }

    /// How many routing rows name `account` for this workspace.
    async fn routed(&self, provider: Provider, account: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT count(*) FROM core.connector_installs \
             WHERE provider = $1 AND external_account_id = $2 AND workspace_id = $3::uuid",
        )
        .bind(provider.id())
        .bind(account)
        .bind(self.workspace.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("the routing rows count")
    }

    /// `…/connectors/{provider}` for this workspace.
    fn one(&self, provider: Provider) -> String {
        format!(
            "/v1/workspaces/{}/connectors/{}",
            self.workspace,
            provider.id()
        )
    }

    /// `…/connectors` for this workspace.
    fn all(&self) -> String {
        format!("/v1/workspaces/{}/connectors", self.workspace)
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the fixture token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Connector status live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'connector-status', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'connector-status@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the tenant, workspace, person and credential seed");
    }

    /// Seals `body` under `provider`'s grant key, replacing whatever was there.
    ///
    /// Through the real vault: a row written by hand would be one the reader
    /// could not open, which answers `not_connected` — passing the negative
    /// cases for the wrong reason and failing the positive one with no clue.
    async fn seal_handle(&self, provider: Provider, body: &str) {
        let name = SecretName::parse(provider.grant_key()).expect("a provider key is storable");
        let raw = serde_json::value::RawValue::from_string(body.to_owned())
            .expect("the fixture handle is an object");
        let vault = harness::vault(self.database.clone());
        let sealed = vault
            .create(
                &self.workspace,
                &name,
                &SecretBody::parse(&raw).expect("the fixture handle is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await;
        assert!(sealed.is_ok(), "the fixture handle seals: {sealed:?}");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid")
            .bind(self.workspace.as_str())
            .execute(&mut *transaction)
            .await
            .expect("the sealed handles clean up");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the tenant cascades away");
        transaction.commit().await.expect("the cleanup commits");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

#[path = "integration_connector_status/checks.rs"]
mod checks;

use checks::*;
