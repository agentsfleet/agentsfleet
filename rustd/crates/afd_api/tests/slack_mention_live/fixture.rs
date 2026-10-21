//! A deployment with the Slack app installed in one workspace, and fleets that
//! workspace attached to a channel.
//!
//! Everything a mention reads on its way to the ledger is real here: the app
//! bag the wall verifies against, the install row the team resolves through,
//! the sealed grant the bot's identity is read from, and fleet rows whose
//! stored configurations are ones Save accepts. Dragonfly is unreachable;
//! admission commits to Postgres and its notification may fail independently.
//!
//! Slack is a loopback. A routed mention reads its thread back before it is
//! admitted, and the connectors' exchange is pinned at [`FakeSlack`] so that
//! read — carrying the fixture grant's bearer — never leaves the machine.

use afd_admission::Producer;
use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{
    FIXTURE_SETUP_MODEL_REF, FleetSeed, InstallSeed, SetupRevision, TestDatabase, WorkspaceSeed,
    mint_id,
};
use afd_dragonfly::Dragonfly;
use afd_fleet_runtime::test_util::FixtureSetup;
use afd_tenant::test_util::{Signup, sign_up};
use afd_vault::{SecretBody, SecretName};

use super::harness;
use afd_connector::test_util::FakeSlack;

/// The field a connector's app bag carries its inbound signing secret in.
const SIGNING_FIELD: &str = "signing_secret";

/// What this deployment's Slack app signs its deliveries with.
pub(super) const SIGNING_SECRET: &[u8] = b"fixture-slack-mention-signing-secret";

/// The bot's own user id, as the grant records it.
pub(super) const BOT_USER: &str = "UBOTFIXTURE";

/// What every fixture person's subject starts with.
const SUBJECT_PREFIX: &str = "user_live_slack_mention_";

/// One admission row, as the ledger holds it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Admitted {
    pub(super) fleet: String,
    pub(super) actor: String,
    pub(super) event_type: String,
    pub(super) request_json: String,
    pub(super) reply_provider: Option<String>,
    pub(super) reply_address: Option<String>,
}

/// A deployment with the Slack app installed for one team.
pub(super) struct Fixture {
    lane: TestDatabase,
    database: Db,
    subject: String,
    tenant: String,
    admin: Uuid7,
    workspace: Uuid7,
    user: String,
    /// The Slack team this fixture's install maps, unique per fixture so two
    /// suites in one database never resolve each other's workspace.
    pub(super) team: String,
    /// Where the router reads a thread back.
    pub(super) slack: FakeSlack,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        Self::create_with(&[]).await
    }

    /// A fixture whose pool is opened with `extra` environment, such as a
    /// pool size a suite needs to prove what one request holds.
    pub(super) async fn create_with(extra: &[(&str, &str)]) -> Self {
        let lane = TestDatabase::shared();
        let tenant = mint_id();
        Self {
            database: lane.open(DbRole::Api, extra).await,
            slack: FakeSlack::start().await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            team: format!("T{}", tenant.replace('-', "").to_ascii_uppercase()),
            tenant,
            admin: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            lane,
        }
    }

    /// The pool this fixture's rows live in.
    pub(super) fn database(&self) -> Db {
        self.database.clone()
    }

    /// The workspace the team resolves to.
    pub(super) const fn workspace(&self) -> &Uuid7 {
        &self.workspace
    }

    /// The production router over this deployment's live stores, its vendor
    /// calls pinned at the loopback Slack.
    pub(super) fn router(&self) -> axum::Router {
        self.instance().router()
    }

    /// The production ingress over this fixture's database, for a case that
    /// drives one of its statements directly rather than through a delivery.
    /// Its vault key and queue are never reached by the statements it runs.
    pub(super) fn ingress(&self) -> afd_ingress::Ingress {
        harness::unreachable_ingress(self.database.clone())
    }

    /// The router with live Dragonfly notifications.
    pub(super) async fn resident_router(&self) -> axum::Router {
        self.instance()
            .with_fleet_queue(self.database.clone(), harness::connect_redis().await)
            .router()
    }

    /// The same router over a scripted ingress, for a ledger state a live
    /// datastore cannot be put in on demand; every other store stays live.
    pub(super) fn scripted_router(
        &self,
        ingress: &std::sync::Arc<harness::Scripted>,
    ) -> axum::Router {
        self.instance().with_ingress(ingress).router()
    }

    fn instance(&self) -> harness::Fleet {
        let queue = Dragonfly::unreachable(&harness::unreachable_queue())
            .expect("a lazy manager opens no socket, so it cannot fail to open one");
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            afd_auth::scope::ScopeSet::from_scopes(&afd_auth::scope::Scope::ALL),
        )
        .with_platform_admin(self.admin.clone())
        .with_live_connectors(self.database.clone(), queue, self.slack.base().to_owned())
    }

    /// The tenant, both workspaces, the person, the app bag, the install row
    /// and the workspace's sealed grant.
    pub(super) async fn seed(&self) {
        sign_up(
            &self.database,
            &Signup {
                tenant: &self.tenant,
                user: &self.user,
                subject: &self.subject,
                email: "mention-live@example.test",
                name: "slack-team",
                display_name: None,
                workspace: self.workspace.as_str(),
            },
        )
        .await;
        let mut connection = self.database.acquire().await.expect("an API connection");
        WorkspaceSeed::new(self.admin.as_str(), &self.tenant)
            .name("platform-admin")
            .insert(&mut *connection)
            .await
            .expect("the admin workspace seeds");
        InstallSeed::new(Provider::Slack.id(), &self.team, self.workspace.as_str())
            .scopes(&["app_mentions:read"])
            .insert(&mut *connection)
            .await
            .expect("the install seeds");
        drop(connection);

        let secret = str::from_utf8(SIGNING_SECRET).expect("the fixture secret is text");
        self.seal(
            &self.admin,
            &Provider::Slack.app_key(),
            &format!(r#"{{"client_id":"fixture","{SIGNING_FIELD}":"{secret}"}}"#),
        )
        .await;
        self.seal(
            &self.workspace,
            Provider::Slack.grant_key(),
            &format!(
                r#"{{"integration":"{}","bot_token":"xoxb-fixture","bot_user_id":"{BOT_USER}","team_id":"{}"}}"#,
                Provider::Slack.id(),
                self.team
            ),
        )
        .await;
    }

    /// A fleet in the team's workspace, stored from the native `config` a
    /// reviewed setup would store, with `status`.
    pub(super) async fn fleet(&self, config: &str, status: &str) -> Uuid7 {
        self.fleet_in(&self.workspace, config, status).await
    }

    /// The same fleet in the tenant's other workspace: the one a Slack team
    /// that moved has left behind.
    pub(super) async fn fleet_elsewhere(&self, config: &str, status: &str) -> Uuid7 {
        self.fleet_in(&self.admin, config, status).await
    }

    async fn fleet_in(&self, workspace: &Uuid7, config: &str, status: &str) -> Uuid7 {
        let parsed = afd_fleet_runtime::FleetConfig::authored(config)
            .expect("the fixture configuration parses");
        let fleet = Uuid7::parse(&mint_id()).expect("a minted fleet is canonical");
        let setup = FixtureSetup::new(config);
        let mut connection = self.database.acquire().await.expect("an API connection");
        FleetSeed::new(fleet.as_str(), workspace.as_str(), &self.tenant, status)
            .name(parsed.name().as_str())
            .at(afd_db::test_util::SEEDED_AT)
            .unplaced_setup(SetupRevision {
                definition: &setup.definition,
                digest: &setup.digest,
                semantics: setup.semantics(),
                model_ref: Some(FIXTURE_SETUP_MODEL_REF),
            })
            .insert(&mut *connection)
            .await
            .expect("the fleet row seeds");
        fleet
    }

    /// The admission a mention key produced, if any.
    pub(super) async fn admission(&self, key: &str) -> Option<Admitted> {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
            ),
        >(
            "SELECT fleet_id::text, actor, event_type, request_json, reply_provider, \
                    reply_address \
             FROM fleet.admissions WHERE producer = $1 AND producer_key = $2",
        )
        .bind(Producer::SlackMention.as_str())
        .bind(key)
        .fetch_optional(&mut *connection)
        .await
        .expect("the admission reads")
        .map(
            |(fleet, actor, event_type, request_json, reply_provider, reply_address)| Admitted {
                fleet,
                actor,
                event_type,
                request_json,
                reply_provider,
                reply_address,
            },
        )
    }

    /// How many mention admissions this fixture's team produced.
    pub(super) async fn admissions(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM fleet.admissions \
             WHERE producer = $1 AND producer_key LIKE $2 || ':%'",
        )
        .bind(Producer::SlackMention.as_str())
        .bind(&self.team)
        .fetch_one(&mut *connection)
        .await
        .expect("the admission count reads")
    }

    async fn seal(&self, workspace: &Uuid7, key: &str, document: &str) {
        let raw = serde_json::value::RawValue::from_string(document.to_owned())
            .expect("the fixture bag is an object");
        let sealed = harness::vault(self.database.clone())
            .create(
                workspace,
                &SecretName::parse(key).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture bag is a storable body"),
                afd_db::test_util::SEEDED_AT,
            )
            .await;
        assert!(sealed.is_ok(), "the fixture bag {key} seals: {sealed:?}");
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = ANY($1::uuid[])")
            .bind(vec![self.admin.as_str(), self.workspace.as_str()])
            .execute(&mut *transaction)
            .await
            .expect("the sealed bags clean up");
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
