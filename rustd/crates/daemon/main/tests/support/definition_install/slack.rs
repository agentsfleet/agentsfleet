//! One signed Slack producer event, delivered to the booted daemon without an ingress stub.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test preconditions fail loudly"
)]

use super::{install_with, seal};
use crate::e2e::{Scenario, unique_ids};
use af_core::id::Uuid7;
use af_webhook::{Scheme, test_util::signature};
use afd_connector::{Provider, test_util::FakeSlack};
use afd_db::test_util::{InstallSeed, WorkspaceSeed};
use agentsfleetd::{
    preflight::{PLATFORM_ADMIN_WORKSPACE_KNOB, SLACK_API_URL_KNOB},
    supervisor::Supervisor,
};
use serde_json::{Value, json};

const CHANNEL: &str = "C0123456789";
const THREAD: &str = "1700000000.000100";
const BOT: &str = "UBOTFIXTURE";
const SECRET: &[u8] = b"fixture-slack-replay-signing-secret";

pub(crate) struct Producer {
    pub(crate) slack: FakeSlack,
    team: String,
    timestamp: String,
    body: String,
    signature: String,
    client: reqwest::Client,
}

impl Producer {
    pub(crate) async fn install(supervisor: &mut Supervisor) -> (Scenario, Self) {
        let slack = FakeSlack::start().await;
        slack.answer(
            THREAD,
            200,
            &json!({"ok": true, "messages": [{"user": "UPERSONFIXTURE", "text": "Please repair this failure", "ts": THREAD}], "has_more": false}).to_string(),
        );
        let admin = unique_ids().0;
        let scene = install_with(
            supervisor,
            "ci-repairer",
            &[],
            None,
            &[
                (SLACK_API_URL_KNOB, &slack.api_base()),
                (PLATFORM_ADMIN_WORKSPACE_KNOB, admin.as_str()),
            ],
            Some(CHANNEL),
        )
        .await;
        let team = format!(
            "T{}",
            scene
                .workspace
                .as_str()
                .replace('-', "")
                .to_ascii_uppercase()
        );
        seed(&scene, &admin, &team).await;
        let timestamp = af_core::clock::now().as_second().to_string();
        let body = json!({"type": "event_callback", "team_id": team, "event_id": "EvSameSignedRepair",
            "event": {"type": "app_mention", "user": "UPERSONFIXTURE", "text": format!("<@{BOT}> ci-repairer: repair https://github.com/agentsfleet/linkwarden/actions/runs/77 attempt 1"),
                "ts": "1700000000.000200", "thread_ts": THREAD, "channel": CHANNEL}}).to_string();
        let signature = signature(Scheme::SlackV0, SECRET, Some(&timestamp), body.as_bytes())
            .expect("the event signs");
        (
            scene,
            Self {
                slack,
                team,
                timestamp,
                body,
                signature,
                client: afd_identity::direct(std::time::Duration::from_secs(30))
                    .build()
                    .expect("a bounded local producer client"),
            },
        )
    }

    pub(crate) async fn deliver(&self, scene: &Scenario) -> Value {
        let response = self
            .client
            .post(format!("{}/v1/connectors/slack/events", scene.base))
            .header("x-slack-request-timestamp", &self.timestamp)
            .header("x-slack-signature", &self.signature)
            .header("content-type", "application/json")
            .body(self.body.clone())
            .send()
            .await
            .expect("the delivery answers");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        response.json().await.expect("the producer receipt is JSON")
    }

    pub(crate) async fn assert_admission(&self, scene: &Scenario, event: &Uuid7) {
        let mut connection = scene
            .booted
            .database
            .acquire()
            .await
            .expect("the scoped pool");
        let rows: Vec<(Uuid7, Uuid7, Uuid7, String)> = sqlx::query_as(
            "SELECT id, fleet_id, workspace_id, request_json FROM fleet.admissions WHERE producer=$1 AND producer_key=$2")
            .bind(afd_turns::Producer::SlackMention.as_str()).bind(format!("{}:EvSameSignedRepair", self.team))
            .fetch_all(&mut *connection).await.expect("the admission reads");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (&rows[0].0, &rows[0].1, &rows[0].2),
            (event, &scene.fleet, &scene.workspace)
        );
        let request: Value = serde_json::from_str(&rows[0].3).expect("the native request is JSON");
        assert_eq!(request["route"]["verdict"], "addressed");
        assert_eq!(request["channel_id"], CHANNEL);
        assert_eq!(request["reply_thread_ts"], THREAD);
        assert_eq!(request["thread"]["fetched"], true);
    }
}

async fn seed(scene: &Scenario, admin: &Uuid7, team: &str) {
    let mut connection = scene
        .booted
        .database
        .acquire()
        .await
        .expect("the scoped pool");
    WorkspaceSeed::new(admin, &scene.tenant)
        .name("platform-app")
        .at(scene.seeded_at)
        .insert(&mut *connection)
        .await
        .expect("the app workspace seeds");
    InstallSeed::new(Provider::Slack.id(), team, &scene.workspace)
        .scopes(&["app_mentions:read"])
        .at(scene.seeded_at)
        .insert(&mut *connection)
        .await
        .expect("the Slack routing row seeds");
    drop(connection);
    seal(
        &scene.booted,
        admin,
        &Provider::Slack.app_key(),
        &json!({"client_id": "fixture", "signing_secret": str::from_utf8(SECRET).expect("text")})
            .to_string(),
        scene.seeded_at,
    )
    .await;
    seal(&scene.booted, &scene.workspace, Provider::Slack.grant_key(),
        &json!({"integration": "slack", "bot_token": "xoxb-fixture", "bot_user_id": BOT, "team_id": team}).to_string(), scene.seeded_at).await;
}
