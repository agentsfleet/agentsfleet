//! The thread a mention is asked in, read back and told to the fleet.
//!
//! Split from the route cases beside it. Slack here is the fixture's loopback,
//! scripted per thread, so each case arranges what Slack holds or how it fails
//! and asserts what the admitted event says: the message the fleet reads, and
//! the `thread` summary beside it.

#![cfg(feature = "test-util")]

use afd_connector::slack::{MAX_MESSAGES, READ_DEADLINE, Unavailable};
use afd_ingress::slack::{THREAD_CAP, THREAD_HEADING, THREAD_UNAVAILABLE};
use serde_json::json;

use super::fake_slack::FakeSlack;
use super::*;

/// What every case asks, once the bot's own mention is removed.
const QUESTION: &str = "why did it fail?";
/// The CI bot that announced the failed run.
const ANNOUNCER: &str = "B0CIBOT01";
/// The run link a CI announcement carries only in its attachment.
const RUN_LINK: &str = "https://github.com/acme/widgets/actions/runs/123";

/// The mention every case delivers.
fn asked() -> String {
    format!("<@{BOT_USER}> {QUESTION}")
}

/// A CI announcement and `replies` replies to it, as Slack answers the read.
fn announced_thread(replies: usize) -> String {
    let parent = json!({
        "ts": THREAD_TS,
        "bot_id": ANNOUNCER,
        "text": "build failed",
        "attachments": [{ "title_link": RUN_LINK }],
    });
    let replies = (1..=replies).map(|reply| {
        json!({
            "ts": format!("1700000001.{reply:06}"),
            "user": PERSON,
            "text": format!("reply {reply}"),
        })
    });
    json!({
        "ok": true,
        "has_more": false,
        "messages": std::iter::once(parent).chain(replies).collect::<Vec<_>>(),
    })
    .to_string()
}

/// A seeded deployment with one read-bound responder attached to the channel.
async fn with_responder(fixture: Fixture) -> (Fixture, axum::Router) {
    fixture.seed().await;
    fixture
        .fleet(
            &document("responder", CHANNEL, Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();
    (fixture, router)
}

/// The admitted event's body for `event_id`.
async fn told(fixture: &Fixture, event_id: &str) -> Value {
    let admitted = fixture
        .admission(&key(&fixture.team, event_id))
        .await
        .expect("the mention was admitted");
    serde_json::from_str(&admitted.request_json).expect("the event body is JSON")
}

/// Dimension 4.1 — a thread of thirty messages is told as the announcement and
/// the latest nineteen replies, parent first and within the cap, and the event
/// says the thread was cut.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn thread_context_keeps_parent_and_latest_replies() {
    let (fixture, router) = with_responder(Fixture::create().await).await;
    fixture.slack.answer(THREAD_TS, 200, &announced_thread(29));

    let body = mention(&fixture.team, "EvThread01", PERSON, &asked());
    assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);

    let request = told(&fixture, "EvThread01").await;
    assert_eq!(
        request.get("thread"),
        Some(&json!({ "fetched": true, "count": MAX_MESSAGES, "truncated": true })),
        "{request}"
    );
    let message = request
        .get("message")
        .and_then(Value::as_str)
        .expect("the event carries a message");
    let (question, thread) = message
        .split_once(&format!("\n\n{THREAD_HEADING}\n"))
        .expect("the thread is told under its heading, after the question");
    assert_eq!(question, QUESTION);
    assert!(thread.chars().count() <= THREAD_CAP, "{message}");

    let lines: Vec<&str> = thread.lines().collect();
    assert_eq!(
        lines.get(..2),
        Some(
            [
                format!("- {ANNOUNCER}: build failed").as_str(),
                format!("  {RUN_LINK}").as_str()
            ]
            .as_slice()
        ),
        "the announcement is told first, its attachment link with it"
    );
    let replies: Vec<&str> = lines
        .iter()
        .filter_map(|line| line.strip_prefix(&format!("- {PERSON}: ")))
        .collect();
    let latest: Vec<String> = (11..=29).map(|reply| format!("reply {reply}")).collect();
    assert_eq!(
        replies, latest,
        "the oldest replies are dropped, the rest told in thread order"
    );
    assert_eq!(fixture.slack.reads(), 1, "one thread, one read");

    fixture.cleanup().await;
}

/// How one case makes the read fail.
type Arrange = fn(&FakeSlack, &str);

/// Dimension 4.3 — a Slack that stalls, one that refuses and one that answers
/// 503 each still admit the mention, told why the thread is missing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn failed_reread_degrades_to_the_mention() {
    let (fixture, router) = with_responder(Fixture::create().await).await;
    let cases: [(&str, &str, Arrange, Unavailable); 3] = [
        (
            "EvThread02",
            "1700000100.000100",
            |slack, ts| slack.stall(ts),
            Unavailable::Timeout,
        ),
        (
            "EvThread03",
            "1700000200.000100",
            |slack, ts| slack.answer(ts, 200, r#"{"ok":false,"error":"not_in_channel"}"#),
            Unavailable::Refused,
        ),
        (
            "EvThread04",
            "1700000300.000100",
            |slack, ts| slack.answer(ts, 503, "{}"),
            Unavailable::Status,
        ),
    ];

    for (event_id, thread_ts, arrange, reason) in cases {
        arrange(&fixture.slack, thread_ts);
        let body = mention_in(&fixture.team, event_id, PERSON, &asked(), thread_ts);
        assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);

        let request = told(&fixture, event_id).await;
        assert_eq!(
            request.get("message").and_then(Value::as_str),
            Some(format!("{QUESTION}\n\n{THREAD_UNAVAILABLE}{}", reason.as_str()).as_str()),
            "{reason:?}"
        );
        assert_eq!(
            request.get("thread"),
            Some(&json!({ "fetched": false, "count": 0, "truncated": false })),
            "{reason:?}"
        );
    }

    fixture.cleanup().await;
}

/// Dimension 4.4 — on a one-connection pool, a second mention is admitted while
/// the first is still waiting on Slack, so the wait holds no connection.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn reread_holds_no_pool_connection() {
    let fixture = Fixture::create_with(&[("DATABASE_POOL_SIZE", "1")]).await;
    let (fixture, router) = with_responder(fixture).await;
    let stalled_thread = "1700000400.000100";
    fixture.slack.stall(stalled_thread);

    let waiting = tokio::spawn({
        let router = router.clone();
        let body = mention_in(
            &fixture.team,
            "EvThread05",
            PERSON,
            &asked(),
            stalled_thread,
        );
        async move { deliver(&router, &body).await }
    });
    fixture.slack.stalled_read().await;

    // A connection held across the wait would park this behind it until the
    // first read's deadline, which is exactly the bound given here.
    let body = mention_in(
        &fixture.team,
        "EvThread06",
        PERSON,
        &asked(),
        "1700000500.000100",
    );
    let second = tokio::time::timeout(READ_DEADLINE, deliver(&router, &body))
        .await
        .expect("a second mention completes while the first waits on Slack");
    assert_eq!(second.status(), StatusCode::OK);
    assert!(
        !waiting.is_finished(),
        "the first mention was still waiting on Slack"
    );

    let first = waiting.await.expect("the waiting delivery completes");
    assert_eq!(first.status(), StatusCode::OK);
    let request = told(&fixture, "EvThread05").await;
    assert_eq!(
        request.get("message").and_then(Value::as_str),
        Some(
            format!(
                "{QUESTION}\n\n{THREAD_UNAVAILABLE}{}",
                Unavailable::Timeout.as_str()
            )
            .as_str()
        )
    );

    fixture.cleanup().await;
}
