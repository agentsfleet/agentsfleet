//! One signed Slack producer event, delivered to the booted daemon without an ingress stub.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test preconditions fail loudly"
)]

use super::{install_with, seal};
use crate::e2e::{Scenario, unique_ids};
use afd_connector::{Provider, test_util::FakeSlack};
use afd_core::id::Uuid7;
use afd_db::test_util::{InstallSeed, WorkspaceSeed};
use afd_webhook::{Scheme, test_util::signature};
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
        let run = install_with(
            supervisor,
            "ci-repairer",
            &[],
            None,
            &[
                (SLACK_API_URL_KNOB, &slack.api_base()),
                (PLATFORM_ADMIN_WORKSPACE_KNOB, &admin),
            ],
            Some(CHANNEL),
        )
        .await;
        let team = format!("T{}", run.workspace.replace('-', "").to_ascii_uppercase());
        seed(&run, &admin, &team).await;
        let timestamp = afd_core::clock::now().as_second().to_string();
        let body = json!({"type": "event_callback", "team_id": team, "event_id": "EvSameSignedRepair",
            "event": {"type": "app_mention", "user": "UPERSONFIXTURE", "text": format!("<@{BOT}> ci-repairer: repair https://github.com/agentsfleet/linkwarden/actions/runs/77 attempt 1"),
                "ts": "1700000000.000200", "thread_ts": THREAD, "channel": CHANNEL}}).to_string();
        let signature = signature(Scheme::SlackV0, SECRET, Some(&timestamp), body.as_bytes())
            .expect("the event signs");
        (
            run,
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

    pub(crate) async fn deliver(&self, run: &Scenario) -> Value {
        let response = self
            .client
            .post(format!("{}/v1/connectors/slack/events", run.base))
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

    pub(crate) async fn assert_admission(&self, run: &Scenario, event: &str) {
        let mut connection = run
            .booted
            .database
            .acquire()
            .await
            .expect("the scoped pool");
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT id::text, fleet_id::text, workspace_id::text, request_json FROM fleet.admissions WHERE producer=$1 AND producer_key=$2")
            .bind(afd_admission::Producer::SlackMention.as_str()).bind(format!("{}:EvSameSignedRepair", self.team))
            .fetch_all(&mut *connection).await.expect("the admission reads");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (&rows[0].0, &rows[0].1, &rows[0].2),
            (&event.to_owned(), &run.fleet, &run.workspace)
        );
        let request: Value = serde_json::from_str(&rows[0].3).expect("the native request is JSON");
        assert_eq!(request["route"]["verdict"], "addressed");
        assert_eq!(request["channel_id"], CHANNEL);
        assert_eq!(request["reply_thread_ts"], THREAD);
        assert_eq!(request["thread"]["fetched"], true);
    }
}

async fn seed(run: &Scenario, admin: &str, team: &str) {
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("the scoped pool");
    WorkspaceSeed::new(admin, &run.tenant)
        .name("platform-app")
        .at(run.seeded_at)
        .insert(&mut *connection)
        .await
        .expect("the app workspace seeds");
    InstallSeed::new(Provider::Slack.id(), team, &run.workspace)
        .scopes(&["app_mentions:read"])
        .at(run.seeded_at)
        .insert(&mut *connection)
        .await
        .expect("the Slack routing row seeds");
    drop(connection);
    let admin = Uuid7::parse(admin).expect("the app workspace id");
    let workspace = Uuid7::parse(&run.workspace).expect("the fixture workspace id");
    seal(
        &run.booted,
        &admin,
        &Provider::Slack.app_key(),
        &json!({"client_id": "fixture", "signing_secret": str::from_utf8(SECRET).expect("text")})
            .to_string(),
        run.seeded_at,
    )
    .await;
    seal(&run.booted, &workspace, Provider::Slack.grant_key(),
        &json!({"integration": "slack", "bot_token": "xoxb-fixture", "bot_user_id": BOT, "team_id": team}).to_string(), run.seeded_at).await;
}
