//! The rows an approval callback is judged against, and the vault it reads.
//!
//! Split from the tests because the seed is the larger half and none of it is
//! the assertion: two fleets, two pending gates identical but for their
//! identifiers, and one deployment secret sealed where the route looks for it.
//!
//! # The second fleet is load-bearing, not a duplicate
//!
//! It carries the two cases one fleet cannot: that a gate is not resolvable
//! through another fleet's path, and that the dashboard's bearer route leaves
//! the same row Slack's callback does. Both need a gate the callback test has
//! not already answered.

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{
    ApiKeySeed, FleetSeed, TenantSeed, TestDatabase, UserSeed, WorkspaceSeed, mint_id,
};
use afd_vault::{SecretBody, SecretName};

use super::PLATFORM_SECRET;
use super::harness;

/// The vault key the approval signing secret is stored under.
///
/// `afd_api_ingress`'s `APPROVAL_IDENTITY` is `pub(crate)` and an integration
/// test is a separate crate, so this reads the public spelling of the same
/// contract — the one the route itself resolves through.
const APPROVAL_KEY: &str = afd_http::services::APPROVAL_IDENTITY;

/// The field a stored webhook credential carries its secret in.
///
/// `afd_ingress`'s `WEBHOOK_SECRET_FIELD`, which is private to the reader. A
/// fixture spelling it differently would store a credential the route opens and
/// then finds empty, which is `UZ-WH-020` — the same answer as storing nothing.
const SECRET_FIELD: &str = "webhook_secret";

/// The expiry the seeded gates carry: 2100-01-01, far past any run, so no test
/// here races the expiry sweeper or a locking validity check into a decision no
/// person made. No assertion reads it back.
const GATE_EXPIRES_AT: &str = "2100-01-01T00:00:00Z";

/// The event type a continuation is written under.
const EVENT_CONTINUATION: &str = "continuation";

/// What every fixture person's subject starts with.
const SUBJECT_PREFIX: &str = "user_live_approval_callback_";

/// Two fleets, two pending gates, and a deployment holding a signing secret.
pub(super) struct Fixture {
    lane: TestDatabase,
    pub(super) database: Db,
    /// Who the dashboard half acts as, and what its `resolved_by` will say.
    ///
    /// Minted per fixture rather than named once for the file, because
    /// `identity.users.oidc_subject` is unique across the whole deployment and the
    /// tests in this file seed their rows in parallel. A shared spelling makes
    /// the first fixture to commit win and every other one fail its seed.
    pub(super) subject: String,
    tenant: String,
    workspace: Uuid7,
    pub(super) other_workspace: Uuid7,
    admin: Uuid7,
    pub(super) fleet: String,
    other_fleet: String,
    user: String,
    key: String,
    pub(super) gate: String,
    pub(super) other_gate: String,
    pub(super) action: String,
    pub(super) other_action: String,
    pub(super) event: String,
    pub(super) other_event: String,
    pub(super) token: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            workspace: minted(),
            other_workspace: minted(),
            admin: minted(),
            fleet: mint_id(),
            other_fleet: mint_id(),
            user: mint_id(),
            key: mint_id(),
            gate: mint_id(),
            other_gate: mint_id(),
            action: mint_id(),
            other_action: mint_id(),
            event: mint_id(),
            other_event: mint_id(),
            token: format!("agt_t{bits}"),
            lane,
        }
    }

    /// The production router over this fixture's rows, its queue and its vault.
    pub(super) async fn router(&self) -> axum::Router {
        let queue = harness::connect_redis().await;
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_owned_workspace(self.other_workspace.clone())
        .with_platform_admin(self.admin.clone())
        .with_approval_queue(self.database.clone(), queue)
        .router()
    }

    pub(super) async fn seed(&self) {
        self.seed_tenant().await;
        self.seed_gates().await;
        self.seal_signing_secret().await;
    }

    /// The tenant, both workspaces, the person, their key, and both fleets.
    async fn seed_tenant(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("Approval callback live")
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        for (workspace, name) in [
            (&self.workspace, "callback"),
            (&self.other_workspace, "dashboard"),
            (&self.admin, "platform-admin"),
        ] {
            WorkspaceSeed::new(workspace.as_str(), &self.tenant)
                .name(name)
                .insert(&mut *connection)
                .await
                .expect("its workspaces seed");
        }
        UserSeed::new(
            &self.user,
            &self.tenant,
            &self.subject,
            "callback-live@example.test",
        )
        .insert(&mut *connection)
        .await
        .expect("its person seeds");
        ApiKeySeed::new(&self.key, &self.tenant, digest.as_str(), &self.subject)
            .insert(&mut *connection)
            .await
            .expect("their API key seeds");
        for (fleet, workspace, name) in [
            (&self.fleet, &self.workspace, "callback"),
            (&self.other_fleet, &self.other_workspace, "dashboard"),
        ] {
            FleetSeed::new(
                fleet,
                workspace.as_str(),
                &self.tenant,
                afd_state::sql::FLEET_STATUS_ACTIVE,
            )
            .name(name)
            .at(afd_db::test_util::SEEDED_AT)
            .native_setup()
            .insert(&mut *connection)
            .await
            .expect("the tenant's fleets seed");
        }
    }

    /// One pending gate on each fleet, differing in nothing but identifiers.
    async fn seed_gates(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO fleet.approval_gates \
               (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_type, \
                proposed_action, evidence, blast_radius, expires_at, resolved_by, status, \
                detail, created_at, updated_at, event_id, spend_count, spend_ceiling) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push', 'tool', \
                     'open a pull request', '{}', 'one repository', $9::timestamptz, '', \
                     'pending', '', now(), now(), $5, 0, 32), \
                    ($6::uuid, $7::uuid, $8::uuid, $10, 'git', 'push', 'tool', \
                     'open a pull request', '{}', 'one repository', $9::timestamptz, '', \
                     'pending', '', now(), now(), $11, 0, 32)",
        )
        .bind(&self.gate)
        .bind(&self.fleet)
        .bind(self.workspace.as_str())
        .bind(&self.action)
        .bind(&self.event)
        .bind(&self.other_gate)
        .bind(&self.other_fleet)
        .bind(self.other_workspace.as_str())
        .bind(GATE_EXPIRES_AT)
        .bind(&self.other_action)
        .bind(&self.other_event)
        .execute(&mut *connection)
        .await
        .expect("both pending gates seed");
    }

    /// The signing secret, sealed where the route reads it from.
    ///
    /// Through the real vault under the harness's own key rather than an INSERT
    /// of ciphertext: a row this fixture hand-wrote would be one the route could
    /// not open, and the route answers `UZ-WH-020` for that — a refusal
    /// indistinguishable from the fixture having stored nothing at all.
    ///
    /// The stored bytes are [`PLATFORM_SECRET`] itself rather than a second
    /// spelling of it, so a test cannot sign under one secret and seal another
    /// and read the resulting refusal as a verdict about the route (RULE TFX).
    async fn seal_signing_secret(&self) {
        let secret = str::from_utf8(PLATFORM_SECRET).expect("the fixture secret is text");
        let document = format!(r#"{{"{SECRET_FIELD}":"{secret}"}}"#);
        let raw = serde_json::value::RawValue::from_string(document)
            .expect("the fixture credential is an object");
        harness::vault(self.database.clone())
            .create(
                &self.admin,
                &SecretName::parse(APPROVAL_KEY).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture credential is a storable body"),
                afd_db::test_util::SEEDED_AT,
            )
            .await
            .expect("the deployment's signing secret seals");
    }

    /// The gate's state and the door that answered it.
    pub(super) async fn gate_state(&self, gate: &str) -> (String, String) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_as(
            "SELECT status, coalesce(resolved_by, '') FROM fleet.approval_gates \
             WHERE id = $1::uuid",
        )
        .bind(gate)
        .fetch_one(&mut *connection)
        .await
        .expect("the seeded gate is readable")
    }

    /// How many continuations resumed `event`.
    ///
    /// Counted by `resumes_event_id` rather than over the fleet's whole
    /// history: a fleet accumulates events for reasons that have nothing to do
    /// with an approval, and a count over all of them would pass on the wrong
    /// row.
    pub(super) async fn continuations(&self, event: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM fleet.events \
             WHERE resumes_event_id = $1 AND event_type = $2",
        )
        .bind(event)
        .bind(EVENT_CONTINUATION)
        .fetch_one(&mut *connection)
        .await
        .expect("the continuation count reads")
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
            .expect("the sealed signing secret cleans up");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
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
