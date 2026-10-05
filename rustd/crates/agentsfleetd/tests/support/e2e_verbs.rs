//! What the runner-verb suites stand up beside a booted daemon: a QStash that
//! records what it is told, a loopback Slack, a Slack grant, and an event
//! admitted from a Slack thread.
//!
//! Split from `e2e.rs` by concern (RULE FLL): that file boots and seeds the
//! scenario every suite shares, and this one adds the two vendors only the
//! schedules and messages verbs reach.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::{Arc, Mutex};

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_connector::Provider;
use afd_connector::test_util::FakeSlack;
use afd_core::id::Uuid7;
use axum::Router;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::post;
use serde_json::{Value, json};
use tokio::task::JoinHandle;

use crate::bundle_install::seal;
use crate::e2e::{ACTOR, REQUEST_JSON, Scenario};
use crate::e2e_event::ensure_group;

/// The bot token a scenario's Slack grant carries.
pub(crate) const BOT_TOKEN: &str = "xoxb-fixture-interim-bot";
/// The bot user the same grant recorded.
pub(crate) const BOT_USER: &str = "U0FIXTUREINTERIM";
/// The channel an admitted Slack event came from.
pub(crate) const CHANNEL: &str = "C0FIXTUREINTERIM";
/// The thread inside it.
pub(crate) const THREAD: &str = "1712345678.000200";

/// The header QStash reads a schedule's expression from.
const HEADER_CRON: &str = "Upstash-Cron";
/// The fixture bearer the daemon presents to the fake scheduler.
pub(crate) const QSTASH_TOKEN: &str = "qstash-fixture-token";

/// What the fake scheduler was told, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Told {
    /// A schedule registered, with the expression it fires on.
    Upsert(String),
    /// A schedule removed, by the key it was registered under.
    Delete(String),
}

/// A QStash that registers whatever it is sent and records it.
pub(crate) struct FakeQStash {
    base: String,
    told: Arc<Mutex<Vec<Told>>>,
    handle: JoinHandle<()>,
}

impl FakeQStash {
    /// Starts one on a loopback port.
    pub(crate) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let base = format!("http://{}/v2", listener.local_addr().expect("bound"));
        let told: Arc<Mutex<Vec<Told>>> = Arc::default();
        let (on_upsert, on_delete) = (Arc::clone(&told), Arc::clone(&told));
        let router = Router::new().route(
            "/v2/schedules/{*rest}",
            post(move |headers: HeaderMap| {
                let told = Arc::clone(&on_upsert);
                async move {
                    let cron = headers
                        .get(HEADER_CRON)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_owned();
                    let mut held = told.lock().expect("the record is never poisoned");
                    held.push(Told::Upsert(cron));
                    axum::Json(json!({"scheduleId": format!("scd_fixture_{}", held.len())}))
                }
            })
            .delete(move |Path(key): Path<String>| {
                let told = Arc::clone(&on_delete);
                async move {
                    told.lock()
                        .expect("the record is never poisoned")
                        .push(Told::Delete(key));
                }
            }),
        );
        let handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("the fake scheduler serves until aborted");
        });
        Self { base, told, handle }
    }

    /// The base the daemon's `QSTASH_URL` names.
    pub(crate) fn base(&self) -> &str {
        &self.base
    }

    /// Everything it was told so far.
    pub(crate) fn told(&self) -> Vec<Told> {
        self.told
            .lock()
            .expect("the record is never poisoned")
            .clone()
    }

    /// How many schedules it registered.
    pub(crate) fn upserts(&self) -> usize {
        self.told()
            .iter()
            .filter(|told| matches!(told, Told::Upsert(_)))
            .count()
    }

    /// How many schedules it removed.
    pub(crate) fn deletes(&self) -> usize {
        self.told()
            .iter()
            .filter(|told| matches!(told, Told::Delete(_)))
            .count()
    }
}

impl Drop for FakeQStash {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Seals a Slack grant for the scenario's workspace, under the daemon's key.
pub(crate) async fn seal_slack_grant(run: &Scenario) {
    let workspace = Uuid7::parse(&run.workspace).expect("a minted workspace id");
    let body = json!({"integration": "slack", "bot_token": BOT_TOKEN, "bot_user_id": BOT_USER});
    seal(
        &run.booted,
        &workspace,
        Provider::Slack.grant_key(),
        &body.to_string(),
        afd_core::clock::now(),
    )
    .await;
}

/// Admits an event on the scenario's fleet from a Slack thread, so an answer
/// or a line said before it is owed there.
pub(crate) async fn enqueue_from_thread(run: &Scenario) -> String {
    ensure_group(&run.booted, &run.fleet).await;
    let address = json!({"team_id": "T0FIXTURE", "channel_id": CHANNEL, "thread_ts": THREAD});
    let address = address.to_string();
    let admitted = Admissions::for_tests(run.booted.database.clone(), run.booted.queue.clone())
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Unrepeatable,
            fleet: &run.fleet,
            workspace: &run.workspace,
            actor: ACTOR,
            event_type: crate::e2e::EVENT_TYPE,
            request_json: REQUEST_JSON,
            reply: Reply::To {
                connector: Provider::Slack.id(),
                address: &address,
            },
        })
        .await
        .expect("the ledger must admit the event");
    admitted.stored.id
}

/// The `chat.postMessage` bodies `slack` received, in order.
pub(crate) fn posts(slack: &FakeSlack) -> Vec<Value> {
    slack
        .requests()
        .into_iter()
        .filter(|request| request.is_post())
        .map(|request| request.body)
        .collect()
}

/// A request to `path` with `method`, carrying the runner's credential and,
/// when given, a JSON body.
pub(crate) async fn send(
    http: &reqwest::Client,
    run: &Scenario,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> reqwest::Response {
    let request = http
        .request(method, format!("{}{path}", run.base))
        .bearer_auth(&run.token);
    let request = match body {
        Some(body) => request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(body).expect("the fixture body serializes")),
        None => request,
    };
    request.send().await.expect("the booted daemon answers")
}
