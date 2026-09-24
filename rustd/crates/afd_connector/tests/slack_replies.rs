//! Reading a Slack thread back, over a socket rather than a string comparison.
//!
//! The pure half — flattening, the window — is unit-tested beside the code.
//! What only a socket shows is the request itself: that it lands on Slack's
//! own path at the pinned host, carries the bot's bearer and the thread's form
//! fields, follows Slack's cursor, and that every way Slack can fail to answer
//! comes back as a reason rather than an error or a hang.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use afd_connector::slack::{self, READ_DEADLINE, Thread, Unavailable};
use afd_crypto::secret::SecretString;
use axum::Router;
use axum::extract::Form;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use tokio::net::TcpListener;

/// The bot token the grant holds.
const TOKEN: &str = "xoxb-fixture-replies";

/// The path Slack serves thread reads on — the vendor's, not a lane's. The fake
/// routes nothing else, so a read that lands elsewhere is a 404 and fails.
const VENDOR_PATH: &str = "/api/conversations.replies";

/// How the fake answers one request, in order.
#[derive(Clone)]
enum Answer {
    /// This status and body.
    Respond(u16, &'static str),
    /// Nothing until well past the deadline: the case under test is a Slack
    /// that never answers, which a closed socket would not be.
    Stall,
}

/// One request as the fake received it.
#[derive(Debug, Clone)]
struct Asked {
    /// The `Authorization` header, verbatim.
    authorization: String,
    /// The form fields, decoded.
    fields: HashMap<String, String>,
}

/// A loopback Slack that answers from a script and records every request.
struct FakeSlack {
    base: String,
    requests: Arc<Mutex<Vec<Asked>>>,
}

impl FakeSlack {
    async fn answering(script: Vec<Answer>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("the listener is bound")
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let turns = Arc::new(AtomicUsize::new(0));
        let script = Arc::new(script);
        let router = Router::new().route(
            VENDOR_PATH,
            post(
                move |headers: HeaderMap, Form(fields): Form<HashMap<String, String>>| {
                    let turn = turns.fetch_add(1, Ordering::SeqCst);
                    recorded
                        .lock()
                        .expect("no test holds this lock across a panic")
                        .push(Asked {
                            authorization: headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default()
                                .to_owned(),
                            fields,
                        });
                    let answer = script.get(turn).cloned();
                    async move { answered(answer).await }
                },
            ),
        );
        tokio::spawn(async move {
            let _served = axum::serve(listener, router).await;
        });
        Self { base, requests }
    }

    fn requests(&self) -> Vec<Asked> {
        self.requests
            .lock()
            .expect("no test holds this lock across a panic")
            .clone()
    }
}

/// The scripted answer, or a 500 for a request the script did not expect.
async fn answered(answer: Option<Answer>) -> Response {
    match answer {
        Some(Answer::Respond(status, body)) => (
            StatusCode::from_u16(status).expect("a scripted status is valid"),
            [(CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response(),
        Some(Answer::Stall) => {
            tokio::time::sleep(READ_DEADLINE * 4).await;
            StatusCode::GATEWAY_TIMEOUT.into_response()
        }
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn thread() -> Thread {
    Thread {
        team_id: Some("T024BE7LD".to_owned()),
        channel_id: "C0123456789".to_owned(),
        thread_ts: "1700000000.000100".to_owned(),
    }
}

fn token() -> SecretString {
    SecretString::new(TOKEN.to_owned())
}

async fn read(fake: &FakeSlack) -> Result<slack::Replies, Unavailable> {
    slack::replies(
        &reqwest::Client::new(),
        Some(&fake.base),
        &token(),
        &thread(),
    )
    .await
}

/// The read lands on Slack's path at the pinned host with the bot's bearer and
/// the thread's fields, follows the cursor to the second page, and answers the
/// parent first.
#[tokio::test]
async fn a_thread_is_read_across_pages_with_the_bots_bearer() {
    let fake = FakeSlack::answering(vec![
        Answer::Respond(
            200,
            r#"{"ok":true,"has_more":true,"response_metadata":{"next_cursor":"dGhlIG5leHQ="},
               "messages":[{"ts":"1700000000.000100","bot_id":"B01","text":"build failed"},
                           {"ts":"1700000000.000200","user":"U01","text":"looking"}]}"#,
        ),
        Answer::Respond(
            200,
            r#"{"ok":true,"has_more":false,"response_metadata":{"next_cursor":""},
               "messages":[{"ts":"1700000000.000300","user":"U02","text":"it is the cache"}]}"#,
        ),
    ])
    .await;

    let replies = read(&fake).await.expect("both pages answered");
    let texts: Vec<&str> = replies.messages.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(texts, ["build failed", "looking", "it is the cache"]);
    assert_eq!(replies.seen, 3);

    let requests = fake.requests();
    assert_eq!(requests.len(), 2, "one request per page");
    for request in &requests {
        assert_eq!(request.authorization, format!("Bearer {TOKEN}"));
        assert_eq!(
            request.fields.get("channel").map(String::as_str),
            Some("C0123456789")
        );
        assert_eq!(
            request.fields.get("ts").map(String::as_str),
            Some("1700000000.000100")
        );
    }
    assert!(
        requests
            .first()
            .is_some_and(|first| !first.fields.contains_key("cursor")),
        "the first page names no cursor"
    );
    assert_eq!(
        requests
            .get(1)
            .and_then(|second| second.fields.get("cursor"))
            .map(String::as_str),
        Some("dGhlIG5leHQ="),
        "the second page starts at Slack's cursor"
    );
}

/// `ok: false`, a 503 and a 200 that is not Slack's answer each come back as
/// their own reason.
#[tokio::test]
async fn every_failed_answer_is_its_own_reason() {
    for (answer, reason) in [
        (
            Answer::Respond(200, r#"{"ok":false,"error":"not_in_channel"}"#),
            Unavailable::Refused,
        ),
        (Answer::Respond(503, "{}"), Unavailable::Status),
        (
            Answer::Respond(200, "<html>captive portal</html>"),
            Unavailable::Unreadable,
        ),
    ] {
        let fake = FakeSlack::answering(vec![answer]).await;
        assert_eq!(read(&fake).await, Err(reason));
    }
}

/// A Slack that never answers is given up on at the deadline, not waited out.
#[tokio::test]
async fn a_slack_that_never_answers_times_out_at_the_deadline() {
    let fake = FakeSlack::answering(vec![Answer::Stall]).await;
    let started = Instant::now();
    assert_eq!(read(&fake).await, Err(Unavailable::Timeout));
    let waited = started.elapsed();
    assert!(waited >= READ_DEADLINE, "{waited:?}");
    assert!(
        waited < READ_DEADLINE + Duration::from_secs(1),
        "{waited:?}"
    );
}

/// A pin that is not a usable origin refuses before anything is dialled —
/// falling back to Slack would send the bearer from a test.
#[tokio::test]
async fn an_unusable_pin_dials_nothing() {
    let fake = FakeSlack::answering(vec![Answer::Respond(200, r#"{"ok":true}"#)]).await;
    let unusable = format!(
        "http://evil.test@{}",
        fake.base.trim_start_matches("http://")
    );
    let answer = slack::replies(
        &reqwest::Client::new(),
        Some(&unusable),
        &token(),
        &thread(),
    )
    .await;
    assert_eq!(answer, Err(Unavailable::Unreachable));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(fake.requests().is_empty(), "nothing was dialled");
}

/// A host nothing listens on is unreachable: a reason, not an error or a hang.
#[tokio::test]
async fn an_absent_slack_is_a_reason() {
    let closed = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port is available");
    let nowhere = format!(
        "http://127.0.0.1:{}",
        closed.local_addr().expect("the listener is bound").port()
    );
    drop(closed);
    let answer = slack::replies(&reqwest::Client::new(), Some(&nowhere), &token(), &thread()).await;
    assert_eq!(answer, Err(Unavailable::Unreachable));
}
