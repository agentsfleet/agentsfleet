//! The rows an approval callback is judged against, and the vault it reads.
//!
//! Split from the tests because the seed is the larger half and none of it is
//! the assertion: two fleets, two pending access cards identical but for their
//! identifiers, and one deployment secret sealed where the route looks for it.
//! An access card is the gate a person approves or denies; its answer moves
//! the fleet's grant and resumes the run that asked.
//!
//! # The second fleet is load-bearing, not a duplicate
//!
//! It carries the two cases one fleet cannot: that a gate is not resolvable
//! through another fleet's path, and that the dashboard's bearer route leaves
//! the same row Slack's callback does. Both need a gate the callback test has
//! not already answered.

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_approval::{GrantKind, IntegrationGrants, Requested, Wanted};
use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{
    ApiKeySeed, FleetSeed, SEEDED_TURN, TenantSeed, TestDatabase, TurnSeed, UserSeed,
    WorkspaceSeed, mint_id,
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

/// The workspace secret each fleet's run asks a person for.
const ASKED_FOR: &str = "sentry";

/// The bearer a signed-in person's browser session presents, which the
/// dashboard half answers with: only a signed-in person approves an access
/// card on the dashboard's route, and an API key is refused there.
pub(super) const SESSION: &str = "fixture.header.payload";

/// The event type a continuation is written under.
const EVENT_CONTINUATION: &str = "continuation";

/// What every fixture person's subject starts with.
const SUBJECT_PREFIX: &str = "user_live_approval_callback_";

/// Two fleets, two pending access cards, and a deployment holding a signing
/// secret.
pub(super) struct Fixture {
    lane: TestDatabase,
    pub(super) database: Db,
    /// The bus the router's approval queue rings.
    bus: afd_bus::Handle,
    /// Who the dashboard half acts as, and what its `resolved_by` will say.
    ///
    /// Minted per fixture rather than named once for the file, because
    /// `identity.users.oidc_subject` is unique across the whole deployment and the
    /// tests in this file seed their rows in parallel. A shared spelling makes
    /// the first fixture to commit win and every other one fail its seed.
    pub(super) subject: String,
    tenant: Uuid7,
    workspace: Uuid7,
    pub(super) other_workspace: Uuid7,
    admin: Uuid7,
    pub(super) fleet: Uuid7,
    pub(super) other_fleet: Uuid7,
    user: Uuid7,
    key: Uuid7,
    /// Each fleet's card and the action a callback names it by. The store mints
    /// them when [`Self::seed`] raises the cards; until then they name nothing.
    pub(super) gate: Uuid7,
    pub(super) other_gate: Uuid7,
    pub(super) action: Uuid7,
    pub(super) other_action: Uuid7,
    pub(super) event: Uuid7,
    pub(super) other_event: Uuid7,
    token: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            bus: harness::bus().await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            workspace: mint_id(),
            other_workspace: mint_id(),
            admin: mint_id(),
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

    /// The production router over this fixture's rows, its bus and its vault.
    ///
    /// The fixture's person is signed in through [`SESSION`] for the dashboard
    /// half, and also holds the fixture's API key.
    pub(super) fn router(&self) -> axum::Router {
        let scopes = ScopeSet::from_scopes(&Scope::ALL);
        harness::Fleet::live(self.database.clone(), &self.subject, scopes)
            .with_dashboard_holding(&self.subject, scopes)
            .with_owned_workspace(self.other_workspace.clone())
            .with_platform_admin(self.admin.clone())
            .with_approval_queue(self.database.clone(), self.bus.clone())
            .router()
    }

    pub(super) async fn seed(&mut self) {
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
            WorkspaceSeed::new(workspace, &self.tenant)
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
                workspace,
                &self.tenant,
                afd_state::sql::FLEET_STATUS_ACTIVE,
            )
            .name(name)
            .at(afd_db::test_util::SEEDED_AT)
            .insert(&mut *connection)
            .await
            .expect("the tenant's fleets seed");
        }
    }

    /// One pending access card on each fleet, differing in nothing but
    /// identifiers: each fleet's run asked for [`ASKED_FOR`] on its turn, through
    /// the store a run's `request_access` writes through.
    async fn seed_gates(&mut self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        // Each card kept a run waiting on a turn, which its answer resumes.
        for (fleet, workspace, turn) in [
            (&self.fleet, &self.workspace, &self.event),
            (&self.other_fleet, &self.other_workspace, &self.other_event),
        ] {
            TurnSeed::new(fleet, workspace, turn, SEEDED_TURN)
                .insert(&mut *connection)
                .await
                .expect("the waiting turn seeds");
        }
        drop(connection);
        let (gate, action) = self.ask(&self.workspace, &self.fleet, &self.event).await;
        let (other_gate, other_action) = self
            .ask(&self.other_workspace, &self.other_fleet, &self.other_event)
            .await;
        (self.gate, self.action) = (gate, action);
        (self.other_gate, self.other_action) = (other_gate, other_action);
    }

    /// `fleet`'s run on `turn` asks for [`ASKED_FOR`], answering the card it
    /// raised and the action a callback names it by.
    async fn ask(&self, workspace: &Uuid7, fleet: &Uuid7, turn: &Uuid7) -> (Uuid7, Uuid7) {
        let requested = IntegrationGrants::new(self.database.clone(), Entropy::new())
            .request(
                workspace,
                fleet,
                Wanted {
                    name: ASKED_FOR,
                    kind: GrantKind::Secret,
                    scope: None,
                    redirect: None,
                    turn,
                },
                af_core::clock::now(),
            )
            .await
            .expect("the fleet's request is answered");
        assert!(
            matches!(requested, Requested::Raised { .. }),
            "a first request raises a card: {requested:?}"
        );
        let gate = requested.gate().cloned().expect("a raised card is named");
        let mut connection = self.database.acquire().await.expect("an API connection");
        let action = sqlx::query_scalar("SELECT action_id FROM fleet.approval_gates WHERE id = $1")
            .bind(&gate)
            .fetch_one(&mut *connection)
            .await
            .expect("the raised card is readable");
        (gate, action)
    }

    /// The status of `fleet`'s grant for [`ASKED_FOR`].
    pub(super) async fn grant_status(&self, fleet: &Uuid7) -> String {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT status FROM connector.integration_grants WHERE fleet_id = $1 AND service = $2",
        )
        .bind(fleet)
        .bind(ASKED_FOR)
        .fetch_one(&mut *connection)
        .await
        .expect("the fleet's grant is readable")
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
    pub(super) async fn gate_state(&self, gate: &Uuid7) -> (String, String) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_as(
            "SELECT status, coalesce(resolved_by, '') FROM fleet.approval_gates \
             WHERE id = $1",
        )
        .bind(gate)
        .fetch_one(&mut *connection)
        .await
        .expect("the seeded gate is readable")
    }

    /// How many continuations resumed `event`.
    ///
    /// Counted by `resumes_turn_id` rather than over the fleet's whole
    /// history: a fleet accumulates events for reasons that have nothing to do
    /// with an approval, and a count over all of them would pass on the wrong
    /// row.
    pub(super) async fn continuations(&self, event: &Uuid7) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM thread.turns \
             WHERE resumes_turn_id = $1 AND kind = $2 AND opened_at IS NOT NULL",
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
        sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1")
            .bind(&self.admin)
            .execute(&mut *transaction)
            .await
            .expect("the sealed signing secret cleans up");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
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
