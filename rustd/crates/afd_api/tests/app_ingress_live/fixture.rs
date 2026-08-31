//! An App installed on an account, and the workspace its deliveries route to.
//!
//! The App surface is the one that cannot be addressed: a provider App posts
//! every event for every repository it can see to ONE endpoint, carrying no
//! fleet, no workspace and no principal. Everything about where a delivery goes
//! is looked up — `core.connector_installs` maps the installation to a
//! workspace, and `core.integration_grants` plus each fleet's stored document
//! decide which fleets in it subscribed.
//!
//! That lookup is `afd_ingress::app`, and it read almost no covered lines,
//! because every suite in front of it injects a stub that answers the routing
//! question from a script. A stub cannot get this wrong; a join can.
//!
//! # Two workspaces, because the surface has two
//!
//! The App's signing secret belongs to the DEPLOYMENT and is read from the
//! platform admin workspace — there is no binding yet when the signature is
//! checked, and there cannot be, since the delivery has to prove itself before
//! it can be routed. The fleets it wakes belong to a TENANT's workspace. One
//! workspace serving both would let a fixture pass while the daemon read the
//! wrong one.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_redis::Redis;
use afd_vault::{SecretBody, SecretName};

use super::harness;

/// The field a stored credential carries its shared secret in.
const WEBHOOK_SECRET_FIELD: &str = "webhook_secret";

/// The vault key the App's own signing secret is held under.
///
/// `app_route.rs`'s `APP_IDENTITY_GITHUB`. A fixture spelling it differently
/// would seal a secret nothing reads, and the wall answers `UZ-WH-020` for that.
const APP_IDENTITY: &str = "github-app";

/// What the App signs its deliveries with.
pub(super) const APP_SECRET: &[u8] = b"fixture-github-app-signing-secret";

/// A secret that is not [`APP_SECRET`].
pub(super) const WRONG_SECRET: &[u8] = b"fixture-github-app-other-secret";

/// The installation the CAPTURED payload names.
///
/// Substituted per fixture by [`Fixture::delivery`], never used as-is:
/// `core.connector_installs` is unique on `(provider, external_account_id)`
/// DEPLOYMENT-wide, so a fixture mapping the captured id would collide with its
/// own leftovers and — worse — let the unmapped case find a mapping some other
/// test wrote. The same trap `core.users.oidc_subject` sets one table over.
const CAPTURED_INSTALLATION: &str = "48765123";

/// The repository the fixture delivery names, from the captured payload.
pub(super) const REPOSITORY: &str = "example/platform";

/// A repository the fixture fleet did not subscribe to.
pub(super) const OTHER_REPOSITORY: &str = "example/unsubscribed";

/// The provider this App belongs to.
const PROVIDER: &str = "github";

/// What every fixture person's subject starts with.
const SUBJECT_PREFIX: &str = "user_live_app_ingress_";

/// Whether this deployment maps the delivery's installation to a workspace.
#[derive(Debug, Clone, Copy)]
pub(super) enum Mapped {
    /// The ordinary state: the installation routes to the tenant workspace.
    ToWorkspace,
    /// An App installed on an account that never finished connecting.
    Nowhere,
}

/// A deployment holding an App, and a tenant whose fleet subscribed to it.
pub(super) struct Fixture {
    lane: TestDatabase,
    database: Db,
    queue: Redis,
    subject: String,
    tenant: String,
    /// Where the App's own signing secret lives.
    admin: Uuid7,
    /// Where the fleets an installation wakes live.
    workspace: Uuid7,
    user: String,
    /// The fleet subscribed to [`REPOSITORY`].
    pub(super) fleet: Uuid7,
    /// This fixture's own installation identifier — see [`CAPTURED_INSTALLATION`].
    installation: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            queue: harness::connect_redis().await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            admin: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            installation: minted_installation(),
            lane,
        }
    }

    /// The production router over both live stores and this deployment's App.
    pub(super) fn router(&self) -> axum::Router {
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            afd_auth::scope::ScopeSet::from_scopes(&afd_auth::scope::Scope::ALL),
        )
        .with_live_ingress(self.database.clone(), self.queue.clone())
        .with_platform_admin(self.admin.clone())
        .router()
    }

    /// Seeds the deployment, the tenant, its fleet, and the install mapping.
    pub(super) async fn seed(&self, mapped: Mapped) {
        self.seed_rows().await;
        if let Mapped::ToWorkspace = mapped {
            self.map_installation().await;
        }
        let text = str::from_utf8(APP_SECRET).expect("the fixture secret is text");
        self.seal(
            APP_IDENTITY,
            &format!(r#"{{"{WEBHOOK_SECRET_FIELD}":"{text}"}}"#),
        )
        .await;
    }

    /// The tenant, both workspaces, the person, the fleet and its grant.
    ///
    /// The grant is what `SELECT_APP_SUBSCRIBERS` joins on: a fleet without an
    /// approved grant for the provider is not a subscriber however its document
    /// reads, which is the relational half of the narrowing.
    async fn seed_rows(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'App ingress live', 1, 1) \
             ), admin AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'platform-admin', $3, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($4::uuid, $1::uuid, 'app-ingress', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($5::uuid, $1::uuid, $3, 'app-ingress@example.test', 1, 1) \
             ), fleet AS ( \
               INSERT INTO core.fleets \
                 (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                  status, created_at, updated_at) \
               VALUES ($6::uuid, $4::uuid, $1::uuid, 'app-ingress-fleet', '# fixture', \
                       $7::jsonb, 'active', 1, 1) \
             ) \
             INSERT INTO core.integration_grants \
               (id, fleet_id, service, status, requested_reason, approved_at, \
                revoked_at, created_at) \
             VALUES ($8::uuid, $6::uuid, $9, 'approved', 'fixture', 1, NULL, 1)",
        )
        .bind(&self.tenant)
        .bind(self.admin.as_str())
        .bind(&self.subject)
        .bind(self.workspace.as_str())
        .bind(&self.user)
        .bind(self.fleet.as_str())
        .bind(document())
        .bind(mint_id())
        .bind(PROVIDER)
        .execute(&mut *connection)
        .await
        .expect("the deployment, tenant, fleet and grant seed");
    }

    /// Maps the delivery's installation to the tenant workspace.
    async fn map_installation(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.connector_installs \
               (id, provider, external_account_id, workspace_id, installed_by, \
                scopes, created_at, updated_at) \
             VALUES ($1::uuid, $2, $3, $4::uuid, $5, ARRAY[]::TEXT[], 1, 1)",
        )
        .bind(mint_id())
        .bind(PROVIDER)
        .bind(&self.installation)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .execute(&mut *connection)
        .await
        .expect("the installation maps to a workspace");
    }

    /// The captured delivery, addressed to THIS fixture's installation.
    pub(super) fn delivery(&self, body: &str) -> String {
        body.replace(CAPTURED_INSTALLATION, &self.installation)
    }

    /// What this fixture's installation is called, for the leak assertion.
    pub(super) fn installation(&self) -> &str {
        &self.installation
    }

    /// Seals `body` into the ADMIN workspace under `key`.
    async fn seal(&self, key: &str, body: &str) {
        let raw = serde_json::value::RawValue::from_string(body.to_owned())
            .expect("the fixture credential is an object");
        let sealed = harness::vault(self.database.clone())
            .create(
                &self.admin,
                &SecretName::parse(key).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture credential is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await;
        assert!(sealed.is_ok(), "the fixture App secret seals: {sealed:?}");
    }

    /// How many events this deployment's fleets have accumulated.
    ///
    /// The negative that makes a drop mean something: a status code cannot show
    /// that nothing was acted on.
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
            .expect("the sealed App secret cleans up");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the tenant cascades away");
        transaction.commit().await.expect("the cleanup commits");
        drop(connection);
        drop(self.database);
        drop(self.lane);
    }
}

/// A unique installation identifier, as the provider's JSON carries one.
///
/// A decimal NUMBER, and the two constraints are both load-bearing. It must be
/// unique because `core.connector_installs` is unique on
/// `(provider, external_account_id)` deployment-wide; and it must not lead with
/// a zero, because it is substituted into `installation.id` — a JSON number,
/// which a leading zero makes unparseable, and the route then answers
/// `UZ-WH-002` for a payload that looks fine to a reader.
///
/// Built from a `UUIDv7`'s RANDOM half rather than its whole: the leading digits
/// of a v7 are a timestamp, so two minted a millisecond apart share them.
fn minted_installation() -> String {
    let minted = mint_id().replace('-', "");
    let random_half = minted.get(20..).unwrap_or("1");
    let value = u64::from_str_radix(random_half, 16).unwrap_or(1);
    format!("{}", value.max(1))
}

/// The stored document: one webhook trigger subscribing to one repository.
///
/// The repository list is what makes this fleet a subscriber. A fleet naming no
/// repository has opted in to nothing on this surface — one App delivery is
/// offered to every fleet in the workspace, so silence must not wake it for
/// another team's repository.
fn document() -> String {
    format!(
        r#"{{
          "name": "app-ingress-fleet",
          "x-agentsfleet": {{
            "triggers": [
              {{"type":"webhook","source":"{PROVIDER}",
                "repositories":["{REPOSITORY}"],"events":["workflow_run"]}}
            ],
            "tools": ["bash"],
            "budget": {{ "daily_dollars": 1.0 }}
          }}
        }}"#
    )
}
