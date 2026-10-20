//! A workspace whose fleet asked a question, and the grant that answers it.
//!
//! Split from the Slack poster cases at the file cap, beside the fake Slack
//! they post to.

use std::sync::Arc;

use afd_connector::{Grants, Provider};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_dragonfly::OutboundDelivery;
use afd_dragonfly::streams::EventId;
use afd_vault::{SecretBody, SecretName, Vault};

use super::{ANSWER, BOT_USER, CHANNEL, THREAD};

/// The key every fixture seals under — the harness's own, not a deployment's.
const FIXTURE_KEK: [u8; 32] = [7u8; 32];

/// Not [`FIXTURE_KEK`]: a vault holding it cannot open the fixture's grants.
const ANOTHER_KEK: [u8; 32] = [9u8; 32];

/// The status the seeded fleet carries.
const FLEET_ACTIVE: &str = "active";

/// A workspace whose fleet asked a question, and the grant that answers it.
pub(super) struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    subject: String,
    user: String,
    fleet: Uuid7,
    event: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            subject: format!("user_live_slack_poster_{}", mint_id()),
            user: mint_id(),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            event: format!("evt_{}", mint_id()),
            lane,
        }
    }

    fn vault(&self) -> Vault {
        Vault::new(
            self.database.clone(),
            Arc::new(Kek::from_bytes(FIXTURE_KEK)),
            Entropy::new(),
        )
    }

    pub(super) fn poster(&self, api_base: &str) -> afd_outbound::SlackPoster {
        self.poster_under(FIXTURE_KEK, api_base)
    }

    /// A poster whose vault holds a different key than the one the grant was
    /// sealed under, as after a key change: every grant read fails to open.
    pub(super) fn poster_under_another_key(&self, api_base: &str) -> afd_outbound::SlackPoster {
        self.poster_under(ANOTHER_KEK, api_base)
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "a fixture client aimed at the suite's loopback Slack fake; afd_identity::direct would add a deadline and drop the environment proxy, neither of which this suite proves"
    )]
    fn poster_under(&self, kek: [u8; 32], api_base: &str) -> afd_outbound::SlackPoster {
        let vault = Vault::new(
            self.database.clone(),
            Arc::new(Kek::from_bytes(kek)),
            Entropy::new(),
        );
        afd_outbound::SlackPoster::new(
            Grants::new(vault, self.database.clone(), Entropy::new()),
            reqwest::Client::new(),
            api_base.to_owned(),
        )
    }

    /// The job the queue would have handed the worker.
    pub(super) fn job(&self) -> OutboundDelivery {
        OutboundDelivery {
            id: EventId::of("1700000000001-0"),
            provider: Provider::Slack.id().to_owned(),
            destination: format!(r#"{{"channel_id":"{CHANNEL}","thread_ts":"{THREAD}"}}"#),
            workspace_id: self.workspace.to_string(),
            fleet_id: self.fleet.to_string(),
            event_id: self.event.clone(),
            answer: ANSWER.to_owned(),
        }
    }

    /// Seeds the tenant, workspace and fleet whose grant the poster opens.
    ///
    /// No event row: where the answer belongs rides the job, and a poster that
    /// still read `fleet.events` would find nothing here and fail.
    pub(super) async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("Slack poster live")
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(self.workspace.as_str(), &self.tenant)
            .name("slack-poster")
            .insert(&mut *connection)
            .await
            .expect("the workspace seeds");
        sqlx::query(
            "INSERT INTO identity.users \
               (id, tenant_id, oidc_subject, email, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, 'slack-poster@example.test', now(), now())",
        )
        .bind(&self.user)
        .bind(&self.tenant)
        .bind(&self.subject)
        .execute(&mut *connection)
        .await
        .expect("the person seeds");
        FleetSeed::new(
            self.fleet.as_str(),
            self.workspace.as_str(),
            &self.tenant,
            FLEET_ACTIVE,
        )
        .name("slack-poster-fleet")
        .insert(&mut *connection)
        .await
        .expect("the fleet seeds");
    }

    /// Seals a Slack grant carrying `token` and the bot user it posts as.
    pub(super) async fn seal_grant(&self, token: &str) {
        self.seal(format!(
            r#"{{"integration":"slack","bot_token":"{token}","bot_user_id":"{BOT_USER}"}}"#
        ))
        .await;
    }

    /// Seals a Slack grant carrying `token` and no bot user, as a grant
    /// written before the connect recorded one is.
    pub(super) async fn seal_grant_naming_no_bot_user(&self, token: &str) {
        self.seal(format!(
            r#"{{"integration":"slack","bot_token":"{token}"}}"#
        ))
        .await;
    }

    async fn seal(&self, body: String) {
        let raw = serde_json::value::RawValue::from_string(body)
            .expect("the fixture handle is an object");
        let sealed = self
            .vault()
            .create(
                &self.workspace,
                &SecretName::parse(Provider::Slack.grant_key())
                    .expect("a provider key is storable"),
                &SecretBody::parse(&raw).expect("the fixture handle is a storable body"),
                jiff::Timestamp::now(),
            )
            .await;
        assert!(sealed.is_ok(), "the fixture grant seals: {sealed:?}");
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
            .expect("the sealed grant cleans up");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
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
