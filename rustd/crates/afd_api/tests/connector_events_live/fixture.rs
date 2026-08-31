//! A deployment holding a connector app, and the vault the wall reads it from.
//!
//! `connector_events_route.rs` proves the two refusals that happen before the
//! vault. Everything past them needs one, because
//! `verified_connector_events` reads the `<provider>-app` bag's
//! `signing_secret` out of the platform admin workspace — so a suite about what
//! a VERIFIED delivery earns cannot exist without a sealed bag.
//!
//! # The bag with no signing secret is its own fixture, not an absence
//!
//! A deployment that connected a provider but never configured its inbound
//! signing secret holds a bag that opens and lacks one field. That is a
//! different state from holding no bag at all, and the daemon answers both
//! `UZ-WH-020` deliberately — which is exactly why a test has to be able to
//! build the first one.

use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_vault::{SecretBody, SecretName};

use super::harness;

/// The field a connector's app bag carries its inbound signing secret in.
///
/// `afd_connector`'s `FIELD_SIGNING_SECRET`, which is private to the reader. A
/// fixture spelling it differently would seal a bag that opens and answers no
/// secret, which the wall reads as unconfigured — passing the refusal tests for
/// the wrong reason and failing the acceptance one with no clue why.
const SIGNING_FIELD: &str = "signing_secret";

/// What this deployment's connector app signs its deliveries with.
pub(super) const SIGNING_SECRET: &[u8] = b"fixture-connector-app-signing-secret";

/// A secret that is not [`SIGNING_SECRET`], for the wrong-key half.
pub(super) const WRONG_SECRET: &[u8] = b"fixture-connector-app-other-secret";

/// What every fixture person's subject starts with — see the callback fixture.
const SUBJECT_PREFIX: &str = "user_live_connector_events_";

/// Whether this deployment configured an inbound signing secret.
#[derive(Debug, Clone, Copy)]
pub(super) enum Configured {
    /// A bag carrying [`SIGNING_SECRET`], which is the ordinary state.
    Signing,
    /// A bag that opens and carries no signing secret at all.
    WithoutSecret,
}

/// A deployment with a platform admin workspace holding a connector app.
pub(super) struct Fixture {
    lane: TestDatabase,
    database: Db,
    subject: String,
    tenant: String,
    admin: Uuid7,
    user: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            admin: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            lane,
        }
    }

    /// The production router over this deployment's live vault.
    ///
    /// The queue stays unreachable, and that is a property of the surface
    /// rather than an omission: this route verifies and answers, and a delivery
    /// it acted on would be the event producer the milestone does not build.
    pub(super) fn router(&self) -> axum::Router {
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            afd_auth::scope::ScopeSet::from_scopes(&afd_auth::scope::Scope::ALL),
        )
        .with_platform_admin(self.admin.clone())
        .router()
    }

    pub(super) async fn seed(&self, configured: Configured) {
        self.seed_rows().await;
        let bag = match configured {
            Configured::Signing => {
                let secret = str::from_utf8(SIGNING_SECRET).expect("the fixture secret is text");
                format!(r#"{{"client_id":"fixture","{SIGNING_FIELD}":"{secret}"}}"#)
            }
            Configured::WithoutSecret => r#"{"client_id":"fixture"}"#.to_owned(),
        };
        self.seal(&Provider::Slack.app_key(), &bag).await;
    }

    /// The tenant, its platform admin workspace, and the person who owns it.
    async fn seed_rows(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Connector events live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'platform-admin', $3, 1) \
             ) \
             INSERT INTO core.users \
               (id, tenant_id, oidc_subject, email, created_at, updated_at) \
             VALUES ($4::uuid, $1::uuid, $3, 'events-live@example.test', 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.admin.as_str())
        .bind(&self.subject)
        .bind(&self.user)
        .execute(&mut *connection)
        .await
        .expect("the tenant and its admin workspace seed");
    }

    /// Seals `document` into the admin workspace under `key`.
    ///
    /// Through the real vault under the harness's own key: a row this fixture
    /// hand-wrote would be one the wall could not open, and the wall answers
    /// `UZ-WH-020` for that — indistinguishable from having stored nothing.
    async fn seal(&self, key: &str, document: &str) {
        let raw = serde_json::value::RawValue::from_string(document.to_owned())
            .expect("the fixture bag is an object");
        let sealed = harness::vault(self.database.clone())
            .create(
                &self.admin,
                &SecretName::parse(key).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture bag is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await;
        assert!(sealed.is_ok(), "the fixture bag {key} seals: {sealed:?}");
    }

    /// How many rows this deployment's fleets have accumulated.
    ///
    /// The negative that makes "acknowledged and dropped" mean something: an
    /// acknowledgement is only correct while nothing acted on the delivery, and
    /// a status code cannot show that.
    pub(super) async fn fleet_events(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM core.fleet_events e \
             JOIN core.fleets f ON f.id = e.fleet_id WHERE f.tenant_id = $1::uuid",
        )
        .bind(&self.tenant)
        .fetch_one(&mut *connection)
        .await
        .expect("the fleet event count reads")
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid")
            .bind(self.admin.as_str())
            .execute(&mut *transaction)
            .await
            .expect("the sealed app bag cleans up");
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
