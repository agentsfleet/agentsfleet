//! A signed Slack mention, through the real route, to one admission.
//!
//! Every store the mention reads is live — the app bag behind the wall, the
//! install row the team resolves through, the grant the bot's identity comes
//! from, and fleet rows stored from real documents — so each case below is
//! the path a production mention takes, stopped only at the queue, which the
//! admission ledger defers past by design.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_connector::Provider;
use afd_connector::slack::Thread;
use afd_fleet_lifecycle::FleetStatus;
use afd_webhook::Scheme;
use http::{HeaderName, Method, StatusCode};
use serde_json::Value;

use self::harness::{json_body, send_with_headers};
#[path = "support/fake_slack.rs"]
mod fake_slack;
#[path = "slack_mention_live/fixture.rs"]
mod fixture;

use self::fixture::{BOT_USER, Fixture, SIGNING_SECRET};

/// The scheme Slack signs deliveries under.
const SCHEME: Scheme = Scheme::SlackV0;
/// The provider these deliveries arrive for.
const PROVIDER: Provider = Provider::Slack;

/// The channel the fleets attach to.
const CHANNEL: &str = "C0123456789";
/// The thread the mention is asked in, and the mention's own timestamp.
const THREAD_TS: &str = "1700000000.000100";
const MENTION_TS: &str = "1700000000.000200";
/// The person asking.
const PERSON: &str = "U0PERSON1";

/// Slack's retry header, which a redelivery carries.
const RETRY_HEADER: &str = "x-slack-retry-num";

fn path() -> String {
    format!("/v1/connectors/{}/events", PROVIDER.id())
}

/// A TRIGGER.md attaching `name` to `channel`, bound to a repository with
/// `access` when given.
fn document(name: &str, channel: &str, access: Option<&str>) -> String {
    let binding = access.map_or_else(String::new, |access| {
        let base = if access == "write" {
            "  repository_base: main\n"
        } else {
            ""
        };
        format!("  repositories: [acme/widgets]\n  repository_access: {access}\n{base}")
    });
    format!(
        "---\nname: {name}\nx-agentsfleet:\n  triggers:\n    - type: mention\n      \
         source: slack\n      channels: [{channel}]\n  tools: []\n  budget:\n    \
         daily_dollars: 1.0\n{binding}---\n"
    )
}

/// One `app_mention` from `team`, asked by `user` with `text`, in the
/// fixture thread.
fn mention(team: &str, event_id: &str, user: &str, text: &str) -> String {
    mention_in(team, event_id, user, text, THREAD_TS)
}

/// The same, asked in the thread rooted at `thread_ts`.
fn mention_in(team: &str, event_id: &str, user: &str, text: &str, thread_ts: &str) -> String {
    format!(
        r#"{{"type":"event_callback","team_id":"{team}","event_id":"{event_id}","event":{{"type":"app_mention","user":"{user}","text":"{text}","ts":"{MENTION_TS}","thread_ts":"{thread_ts}","channel":"{CHANNEL}"}}}}"#
    )
}

/// One delivery of `body` signed with `secret` at `at`, plus `extra` headers.
async fn deliver_at(
    router: &axum::Router,
    secret: &[u8],
    at: &str,
    body: &str,
    extra: &[(HeaderName, &str)],
) -> axum::response::Response {
    let proof = harness::webhook::signature_at(SCHEME, secret, Some(at), body.as_bytes());
    let mut headers = vec![
        (name(SCHEME.signature_header()), proof.as_str()),
        (
            name(
                SCHEME
                    .timestamp_header()
                    .expect("the timestamped scheme names its timestamp header"),
            ),
            at,
        ),
    ];
    headers.extend(extra.iter().cloned());
    send_with_headers(router, Method::POST, &path(), None, body, &headers).await
}

/// One delivery of `body`, signed correctly at the router's own instant.
async fn deliver(router: &axum::Router, body: &str) -> axum::response::Response {
    let at = harness::frozen_unix_seconds().to_string();
    deliver_at(router, SIGNING_SECRET, &at, body, &[]).await
}

fn name(header: &str) -> HeaderName {
    HeaderName::from_bytes(header.as_bytes()).expect("the header names are well formed")
}

/// The key a mention from `team` with `event_id` is admitted under.
fn key(team: &str, event_id: &str) -> String {
    format!("{team}:{event_id}")
}

/// Dimension 1.1 — a signed mention in a thread admits one event on the
/// channel's only fleet: keyed by Slack's event id, as the person, a chat, owed
/// back to that thread, with the bot's mention removed from the message.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn signed_mention_admits_one_event() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let responder = fixture
        .fleet(
            &document("responder", CHANNEL, Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();

    let body = mention(
        &fixture.team,
        "Ev01",
        PERSON,
        &format!("<@{BOT_USER}> why did it fail?"),
    );
    let answered = deliver(&router, &body).await;
    assert_eq!(answered.status(), StatusCode::OK);
    let document = json_body(answered).await;
    assert_eq!(
        document.get("replayed").and_then(Value::as_bool),
        Some(false),
        "{document}"
    );

    let admitted = fixture
        .admission(&key(&fixture.team, "Ev01"))
        .await
        .expect("the mention was admitted");
    assert_eq!(admitted.fleet, responder.as_str());
    assert_eq!(admitted.actor, format!("{}:{PERSON}", PROVIDER.id()));
    assert_eq!(admitted.event_type, "chat");
    assert_eq!(admitted.reply_provider.as_deref(), Some(PROVIDER.id()));
    let thread = admitted
        .reply_address
        .as_deref()
        .and_then(Thread::parse)
        .expect("the reply address names a thread");
    assert_eq!(thread.channel_id, CHANNEL);
    assert_eq!(
        thread.thread_ts, THREAD_TS,
        "answered in the thread it was asked in"
    );
    let request: Value =
        serde_json::from_str(&admitted.request_json).expect("the event body is JSON");
    assert_eq!(
        request.get("message").and_then(Value::as_str),
        Some("why did it fail?"),
        "the bot's own mention is not part of the question"
    );
    assert_eq!(
        request.pointer("/route/verdict").and_then(Value::as_str),
        Some("sole")
    );

    fixture.cleanup().await;
}

/// Dimension 1.2 — Slack's retry of the same event admits nothing new and
/// answers the first event, reported as replayed.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn slack_retry_admits_nothing_new() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &document("responder", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();
    let body = mention(
        &fixture.team,
        "Ev02",
        PERSON,
        &format!("<@{BOT_USER}> status?"),
    );

    let first = json_body(deliver(&router, &body).await).await;
    let at = harness::frozen_unix_seconds().to_string();
    let retried = json_body(
        deliver_at(
            &router,
            SIGNING_SECRET,
            &at,
            &body,
            &[(name(RETRY_HEADER), "1")],
        )
        .await,
    )
    .await;
    assert_eq!(
        first.get("event_id"),
        retried.get("event_id"),
        "{first} / {retried}"
    );
    assert_eq!(retried.get("replayed").and_then(Value::as_bool), Some(true));
    assert_eq!(
        fixture.admissions().await,
        1,
        "one Slack event, one admission"
    );

    fixture.cleanup().await;
}

#[path = "integration_slack_mention/drops.rs"]
mod drops;

#[path = "integration_slack_mention/subscribers.rs"]
mod subscribers;

#[path = "integration_slack_mention/notice.rs"]
mod notice;

#[path = "integration_slack_mention/resident.rs"]
mod resident;

#[path = "integration_slack_mention/thread.rs"]
mod thread;
