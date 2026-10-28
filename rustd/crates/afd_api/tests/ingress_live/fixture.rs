//! A fleet that takes signed deliveries, and the live stores it resolves through.
//!
//! Every other signed-ingress suite runs on `Scripted`, which answers the three
//! questions from a script a test wrote. That proves what the HANDLER decides
//! and deliberately proves nothing about the store underneath it — which is why
//! `afd_http`'s production `impl WebhookIngress for Ingress` and the whole of
//! `afd_ingress`'s app resolution read zero covered lines: nothing runs them.
//!
//! This fixture is the other half. The router is the production one over a live
//! Postgres and a live Dragonfly, so a delivery here resolves its binding out of
//! `fleet.fleets`, opens its secret out of `vault.secrets`, and lands its claim
//! in the queue exactly as the daemon does it.

use afd_core::id::Uuid7;
use afd_core::money::NANOS_PER_USD;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FIXTURE_SETUP_MODEL_REF, FleetSeed, SetupRevision, TestDatabase, mint_id};
use afd_dragonfly::Dragonfly;
use afd_fleet_runtime::test_util::FixtureSetup;
use afd_tenant::test_util::{Signup, sign_up};
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
    /// The `fleet.fleets.status` value this state stores as.
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
    queue: Dragonfly,
    subject: String,
    tenant: String,
    pub(super) workspace: Uuid7,
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
            self.seal(
                "clerk",
                &serde_json::json!({"webhook_secret": harness::webhook::SVIX_SECRET}).to_string(),
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
        sign_up(
            &self.database,
            &Signup {
                tenant: &self.tenant,
                user: &self.user,
                subject: &self.subject,
                email: "ingress-live@example.test",
                name: "ingress-live",
                display_name: None,
                workspace: self.workspace.as_str(),
            },
        )
        .await;
        let setup = FixtureSetup::new(&document());
        FleetSeed::new(
            self.fleet.as_str(),
            self.workspace.as_str(),
            &self.tenant,
            runnable.stored(),
        )
        .name("ingress-live-fleet")
        .unplaced_setup(SetupRevision {
            definition: &setup.definition,
            digest: &setup.digest,
            semantics: setup.semantics(),
            model_ref: Some(FIXTURE_SETUP_MODEL_REF),
        })
        .insert(&mut *self.database.acquire().await.expect("an API connection"))
        .await
        .expect("the fleet seeds");
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
                afd_db::test_util::SEEDED_AT,
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
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the tenant cascades away");
        transaction.commit().await.expect("the cleanup commits");
        drop(connection);
        drop(self.lane);
    }
}

/// The second verifiable source the stored document declares.
pub(super) const SECOND_SOURCE: &str = "linear";

/// Registry and declared recipes sharing one stored fleet.
fn document() -> String {
    format!(
        r#"{{
          "name": "ingress-live-fleet",
          "x-agentsfleet": {{
            "triggers": [
              {{"type":"webhook","source":"{SOURCE}","credential_name":"{CREDENTIAL}"}},
              {{"type":"webhook","source":"{SECOND_SOURCE}","credential_name":"{CREDENTIAL}"}},
              {{"type":"webhook","source":"clerk","verifier":{{"scheme":"svix-v1"}}}},
              {{"type":"webhook","source":"acme","credential_name":"{CREDENTIAL}","verifier":{{"scheme":"body-hex-sha256","header":"x-acme-signature"}}}}
            ],
            "tools": ["bash"],
            "budget": {{ "daily_nanos": "{NANOS_PER_USD}" }}
          }}
        }}"#
    )
}
