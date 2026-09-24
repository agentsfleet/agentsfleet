//! A FAKE Slack Web API, on a loopback port, answering thread reads.
//!
//! Named beside `fake_provider.rs` for the same reason: it serves a fixture's
//! answers, and a reader who took it for a real client would look here for the
//! daemon's own thread reader, which lives in `afd_connector::slack`.
//!
//! A mention re-reads its thread before it is admitted, so every suite whose
//! router admits one needs something at Slack's path — and no test may send
//! even a fixture's bearer to Slack. It is scripted by THREAD, the `ts` the
//! read posts, so a suite arranges Slack's state ("this thread holds thirty
//! messages", "this one never answers") rather than an order of calls. A
//! thread nobody scripted answers as an empty one, which is what a mention that
//! starts its own thread reads.
//!
//! The fixture answers bytes and lets the daemon do the reading, the argument
//! `fake_provider.rs` makes: the field names and the `ok` flag are the half that
//! breaks when a vendor changes shape.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use afd_connector::slack::READ_DEADLINE;
use axum::Router;
use axum::extract::Form;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// The path Slack serves thread reads on — the vendor's own, which the
/// daemon's pinned exchange keeps.
const REPLIES_PATH: &str = "/api/conversations.replies";
/// The form field naming the thread's root.
const FIELD_TS: &str = "ts";
/// What an unscripted thread answers: Slack's answer for a thread holding
/// nothing but its root, which the daemon drops as the question itself.
const EMPTY_THREAD: &str = r#"{"ok":true,"messages":[]}"#;
/// What Slack's answers are typed as.
const MEDIA_JSON: &str = "application/json";

/// How one thread answers.
#[derive(Debug, Clone)]
enum Script {
    /// This status and body.
    Answers(u16, String),
    /// Nothing until well past the reader's deadline.
    Stalls,
}

/// State the handler and the test share.
#[derive(Debug, Default)]
struct Shared {
    scripts: Mutex<HashMap<String, Script>>,
    reads: AtomicUsize,
    /// Signalled when a stalled read has arrived, so a test can act while it
    /// is known to be in flight rather than guessing with a sleep.
    stalled: Notify,
}

pub(crate) struct FakeSlack {
    base: String,
    shared: Arc<Shared>,
    handle: JoinHandle<()>,
}

impl FakeSlack {
    pub(crate) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().expect("a bound address"));
        let shared = Arc::new(Shared::default());
        let serving = Arc::clone(&shared);
        let router = Router::new().route(
            REPLIES_PATH,
            post(move |Form(fields): Form<HashMap<String, String>>| {
                let shared = Arc::clone(&serving);
                async move { answer(&shared, fields.get(FIELD_TS)).await }
            }),
        );
        let handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("the fake Slack serves until aborted");
        });
        Self {
            base,
            shared,
            handle,
        }
    }

    /// The origin the daemon's exchange is pinned at.
    pub(crate) fn base(&self) -> String {
        self.base.clone()
    }

    /// `thread_ts` answers `status` with `body`.
    pub(crate) fn answer(&self, thread_ts: &str, status: u16, body: &str) {
        self.script(thread_ts, Script::Answers(status, body.to_owned()));
    }

    /// `thread_ts` never answers within the reader's deadline.
    pub(crate) fn stall(&self, thread_ts: &str) {
        self.script(thread_ts, Script::Stalls);
    }

    /// Resolves once a read of a stalled thread has arrived.
    pub(crate) async fn stalled_read(&self) {
        self.shared.stalled.notified().await;
    }

    /// How many thread reads have arrived.
    pub(crate) fn reads(&self) -> usize {
        self.shared.reads.load(Ordering::SeqCst)
    }

    fn script(&self, thread_ts: &str, script: Script) {
        self.shared
            .scripts
            .lock()
            .expect("no test holds this lock across a panic")
            .insert(thread_ts.to_owned(), script);
    }
}

impl Drop for FakeSlack {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// The scripted answer for the thread `ts` names.
async fn answer(shared: &Shared, ts: Option<&String>) -> Response {
    shared.reads.fetch_add(1, Ordering::SeqCst);
    let script = ts.and_then(|ts| {
        shared
            .scripts
            .lock()
            .expect("no test holds this lock across a panic")
            .get(ts)
            .cloned()
    });
    match script {
        Some(Script::Answers(status, body)) => json(
            StatusCode::from_u16(status).expect("a scripted status is valid"),
            body,
        ),
        Some(Script::Stalls) => {
            shared.stalled.notify_one();
            tokio::time::sleep(READ_DEADLINE * 4).await;
            StatusCode::GATEWAY_TIMEOUT.into_response()
        }
        None => json(StatusCode::OK, EMPTY_THREAD.to_owned()),
    }
}

fn json(status: StatusCode, body: String) -> Response {
    (status, [(CONTENT_TYPE, MEDIA_JSON)], body).into_response()
}
