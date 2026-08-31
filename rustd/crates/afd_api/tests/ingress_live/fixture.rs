//! A fleet that takes signed deliveries, and the live stores it resolves through.
//!
//! Every other signed-ingress suite runs on `Scripted`, which answers the three
//! questions from a script a test wrote. That proves what the HANDLER decides
//! and deliberately proves nothing about the store underneath it — which is why
//! `afd_http`'s production `impl WebhookIngress for Ingress` and the whole of
//! `afd_ingress`'s app resolution read zero covered lines: nothing runs them.
//!
//! This fixture is the other half. The router is the production one over a live
//! Postgres and a live Redis, so a delivery here resolves its binding out of
//! `core.fleets`, opens its secret out of `vault.secrets`, and lands its claim
//! in the queue exactly as the daemon does it.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_redis::Redis;
use afd_vault::{SecretBody, SecretName};

use super::harness;

/// The field a stored credential carries its shared secret in.
///
/// `afd_ingress`'s `WEBHOOK_SECRET_FIELD`, which is private to that reader.
/// Spelling it differently here would seal a credential that opens and answers
/// no secret — which the wall reads as unconfigured, passing a refusal test for
/// the wrong reason.
const WEBHOOK_SECRET_FIELD: &str = "webhook_secret";

/// The vault key the fixture fleet's trigger names.
pub(super) const CREDENTIAL: &str = "ingress-live-credential";

/// What this fleet's provider signs its deliveries with.
pub(super) const SIGNING_SECRET: &[u8] = b"fixture-ingress-live-signing-secret";

/// A secret that is not [`SIGNING_SECRET`], for the wrong-key half.
pub(super) const WRONG_SECRET: &[u8] = b"fixture-ingress-live-other-secret";

/// The provider the fixture fleet's trigger declares.
pub(super) const SOURCE: &str = "github";

/// What every fixture person's subject starts with.
const SUBJECT_PREFIX: &str = "user_live_ingress_";

/// Whether the fixture fleet will take new work.
#[derive(Debug, Clone, Copy)]
pub(super) enum Runnable {
    /// The ordinary state: a delivery runs the fleet.
    Active,
    /// Paused on purpose — a delivery is acknowledged and dropped.
    Paused,
}

impl Runnable {
    /// The `core.fleets.status` value this state stores as.
    const fn stored(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
        }
    }
}

/// Whether this workspace holds a usable secret for the fleet's trigger.
#[derive(Debug, Clone, Copy)]
pub(super) enum Secret {
    /// A credential carrying [`SIGNING_SECRET`].
    Stored,
    /// No credential at all, which is the unconfigured state.
    Absent,
}

/// A workspace holding one fleet that takes signed deliveries.
pub(super) struct Fixture {
    lane: TestDatabase,
    database: Db,
    queue: Redis,
    subject: String,
    tenant: String,
    workspace: Uuid7,
    user: String,
    /// The fleet a delivery is addressed to.
    pub(super) fleet: Uuid7,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            queue: harness::connect_redis().await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            lane,
        }
    }

    /// The production router, resolving through both live stores.
    pub(super) fn router(&self) -> axum::Router {
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            afd_auth::scope::ScopeSet::from_scopes(&afd_auth::scope::Scope::ALL),
        )
        .with_live_ingress(self.database.clone(), self.queue.clone())
        .with_platform_admin(self.workspace.clone())
        .router()
    }

    /// Seeds the tenant, its workspace, the person, and one fleet.
    pub(super) async fn seed(&self, runnable: Runnable, secret: Secret) {
        self.seed_rows(runnable).await;
        if let Secret::Stored = secret {
            let text = str::from_utf8(SIGNING_SECRET).expect("the fixture secret is text");
            self.seal(
                CREDENTIAL,
                &format!(r#"{{"{WEBHOOK_SECRET_FIELD}":"{text}"}}"#),
            )
            .await;
        }
    }

    /// The tenant, its workspace, its person, and the fleet a delivery names.
    ///
    /// The document declares ONE webhook trigger naming [`SOURCE`] and
    /// [`CREDENTIAL`], because that is what makes the row resolve to a binding
    /// at all — a fleet whose document declares no webhook trigger is
    /// `Ok(None)` and answers `UZ-WH-001`, which is a different test.
    async fn seed_rows(&self, runnable: Runnable) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Ingress live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'ingress-live', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'ingress-live@example.test', 1, 1) \
             ) \
             INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             VALUES ($5::uuid, $2::uuid, $1::uuid, 'ingress-live-fleet', '# fixture', \
                     $6::jsonb, $7, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(&self.user)
        .bind(self.fleet.as_str())
        .bind(document())
        .bind(runnable.stored())
        .execute(&mut *connection)
        .await
        .expect("the tenant, workspace, person and fleet seed");
    }

    /// Seals `body` into this workspace under `key`, through the real vault.
    ///
    /// A row hand-written here would be one the reader could not open, and the
    /// wall answers `UZ-WH-020` for that — indistinguishable from having stored
    /// nothing, so the acceptance test would pass for the wrong reason.
    async fn seal(&self, key: &str, body: &str) {
        let raw = serde_json::value::RawValue::from_string(body.to_owned())
            .expect("the fixture credential is an object");
        let sealed = harness::vault(self.database.clone())
            .create(
                &self.workspace,
                &SecretName::parse(key).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture credential is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await;
        assert!(sealed.is_ok(), "the fixture credential seals: {sealed:?}");
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid")
            .bind(self.workspace.as_str())
            .execute(&mut *transaction)
            .await
            .expect("the sealed credential cleans up");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the tenant cascades away");
        transaction.commit().await.expect("the cleanup commits");
        drop(connection);
        drop(self.lane);
    }
}

/// The stored document, declaring one webhook trigger.
fn document() -> String {
    format!(
        r#"{{
          "name": "ingress-live-fleet",
          "x-agentsfleet": {{
            "triggers": [
              {{"type":"webhook","source":"{SOURCE}","credential_name":"{CREDENTIAL}"}}
            ],
            "tools": ["bash"],
            "budget": {{ "daily_dollars": 1.0 }}
          }}
        }}"#
    )
}
