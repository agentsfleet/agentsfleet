//! A workspace whose fleet asked a question, and the grant that answers it.
//!
//! Split from the Slack poster cases at the file cap, beside the fake Slack
//! they post to.

use std::sync::Arc;

use afd_connector::{Grants, Provider};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_dragonfly::OutboundDelivery;
use afd_dragonfly::streams::EventId;
use afd_vault::{SecretBody, SecretName, Vault};

use super::{ANSWER, BOT_USER, CHANNEL, THREAD};

/// The key every fixture seals under — the harness's own, not a deployment's.
const FIXTURE_KEK: [u8; 32] = [7u8; 32];

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
        afd_outbound::SlackPoster::new(
            Grants::new(self.vault(), self.database.clone(), Entropy::new()),
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
    /// still read `core.fleet_events` would find nothing here and fail.
    pub(super) async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Slack poster live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'slack-poster', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'slack-poster@example.test', 1, 1) \
             ) \
             INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             VALUES ($5::uuid, $2::uuid, $1::uuid, 'slack-poster-fleet', '# fixture', \
                     '{}'::jsonb, 'active', 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(&self.user)
        .bind(self.fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the tenant, workspace and fleet seed");
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
                UnixMillis::from_millis(1),
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
