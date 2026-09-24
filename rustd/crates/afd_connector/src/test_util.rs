//! A FAKE Slack Web API on a loopback port, for every suite that reads a
//! thread or posts an answer.
//!
//! One fake for the three suites that need Slack: this crate's thread reader,
//! `afd_outbound`'s poster, and `afd_api`'s mention router. No test may send
//! even a fixture's bearer to Slack, and three copies of one loopback server
//! had already started to answer the same case three ways.
//!
//! It serves the vendor's own paths, spelled here rather than borrowed from
//! the daemon's constants, so a daemon that dials the wrong method is a 404
//! and fails. It answers bytes and lets the daemon do the reading: the field
//! names and the `ok` flag are the half that breaks when a vendor changes
//! shape.
//!
//! # Two ways to script a thread read
//!
//! - [`FakeSlack::start`] scripts by THREAD, the `ts` the read posts, so a
//!   suite arranges Slack's state ("this thread holds thirty messages", "this
//!   one never answers") rather than an order of calls. A thread nobody
//!   scripted answers as an empty one, which is what a mention that starts
//!   its own thread reads.
//! - [`FakeSlack::in_order`] answers each read from a list, in arrival order,
//!   which is how one thread's pages are scripted. A read past the end of the
//!   list is a 500, so a reader that asks once too often fails.
//!
//! Every post to `chat.postMessage` answers what [`FakeSlack::post_answers`]
//! last set, `200 {"ok":true}` until then.

#![expect(
    clippy::expect_used,
    reason = "a test fixture whose precondition fails should stop the suite loudly"
)]

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::Router;
use axum::extract::Form;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse as _, Response};
use axum::routing::post;
use serde_json::Value;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::slack::READ_DEADLINE;

/// The path Slack serves thread reads on — the vendor's own, which a pinned
/// exchange keeps.
const REPLIES_PATH: &str = "/api/conversations.replies";
/// The path Slack serves posts on.
const POST_MESSAGE_PATH: &str = "/api/chat.postMessage";
/// Where Slack's methods hang off the origin, as `SLACK_API_BASE` spells it.
const API_ROOT: &str = "/api";
/// The form field naming the thread's root.
const FIELD_TS: &str = "ts";
/// What an unscripted thread answers: Slack's answer for a thread holding
/// nothing but its root, which the daemon drops as the question itself.
const EMPTY_THREAD: &str = r#"{"ok":true,"messages":[]}"#;
/// What a post answers until a suite says otherwise.
const POST_ACCEPTED: &str = r#"{"ok":true}"#;
/// What Slack's answers are typed as.
const MEDIA_JSON: &str = "application/json";

/// How one read answers.
#[derive(Debug, Clone)]
pub enum Reply {
    /// This status and body.
    Answers(u16, String),
    /// Nothing until well past the reader's deadline: a Slack that never
    /// answers, which a closed socket would not be.
    Stalls,
}

impl Reply {
    /// `status` with `body`.
    #[must_use]
    pub fn answers(status: u16, body: &str) -> Self {
        Self::Answers(status, body.to_owned())
    }
}

/// One request as the fake received it, on either path.
#[derive(Debug, Clone)]
pub struct Request {
    /// Which method it called: [`Request::is_post`] tells them apart.
    path: &'static str,
    /// The `Authorization` header, verbatim.
    pub authorization: String,
    /// The form fields a read posted, or the string fields of a post's JSON
    /// body.
    pub fields: HashMap<String, String>,
    /// A post's whole JSON body; `Null` for a thread read.
    pub body: Value,
}

impl Request {
    /// Whether this was a `chat.postMessage` rather than a thread read.
    #[must_use]
    pub fn is_post(&self) -> bool {
        self.path == POST_MESSAGE_PATH
    }

    /// One field, by name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

/// State the handlers and the test share.
#[derive(Debug)]
struct Shared {
    /// Thread scripts, by `ts`, for [`FakeSlack::start`].
    by_thread: Mutex<HashMap<String, Reply>>,
    /// The ordered script, for [`FakeSlack::in_order`]; `None` scripts by
    /// thread instead.
    in_order: Mutex<Option<VecDeque<Reply>>>,
    /// What every post answers.
    post: Mutex<(u16, String)>,
    requests: Mutex<Vec<Request>>,
    reads: AtomicUsize,
    /// Signalled when a stalled read has arrived, so a test can act while it
    /// is known to be in flight rather than guessing with a sleep.
    stalled: Notify,
}

/// A loopback Slack, aborted when dropped.
#[derive(Debug)]
pub struct FakeSlack {
    base: String,
    shared: Arc<Shared>,
    handle: JoinHandle<()>,
}

impl FakeSlack {
    /// A fake whose thread reads are scripted by thread.
    ///
    /// # Panics
    /// When no loopback port can be bound — a test precondition.
    pub async fn start() -> Self {
        Self::serving(None).await
    }

    /// A fake whose thread reads answer `replies` in arrival order.
    ///
    /// # Panics
    /// When no loopback port can be bound — a test precondition.
    pub async fn in_order(replies: Vec<Reply>) -> Self {
        Self::serving(Some(replies.into())).await
    }

    async fn serving(in_order: Option<VecDeque<Reply>>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("the listener is bound")
        );
        let shared = Arc::new(Shared {
            by_thread: Mutex::default(),
            in_order: Mutex::new(in_order),
            post: Mutex::new((StatusCode::OK.as_u16(), POST_ACCEPTED.to_owned())),
            requests: Mutex::default(),
            reads: AtomicUsize::new(0),
            stalled: Notify::new(),
        });
        let reading = Arc::clone(&shared);
        let posting = Arc::clone(&shared);
        let router = Router::new()
            .route(
                REPLIES_PATH,
                post(
                    move |headers: HeaderMap, Form(fields): Form<HashMap<String, String>>| {
                        let shared = Arc::clone(&reading);
                        async move { read(&shared, &headers, fields).await }
                    },
                ),
            )
            .route(
                POST_MESSAGE_PATH,
                post(move |headers: HeaderMap, sent: String| {
                    let shared = Arc::clone(&posting);
                    async move { posted(&shared, &headers, &sent) }
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

    /// The origin, for an exchange pinned here: a pin keeps the vendor's own
    /// path.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The API root a poster is handed, in place of `SLACK_API_BASE`.
    #[must_use]
    pub fn api_base(&self) -> String {
        format!("{}{API_ROOT}", self.base)
    }

    /// `thread_ts` answers `status` with `body`.
    pub fn answer(&self, thread_ts: &str, status: u16, body: &str) {
        self.script(thread_ts, Reply::answers(status, body));
    }

    /// `thread_ts` never answers within the reader's deadline.
    pub fn stall(&self, thread_ts: &str) {
        self.script(thread_ts, Reply::Stalls);
    }

    /// Every post from now on answers `status` with `body`.
    pub fn post_answers(&self, status: u16, body: &str) {
        *locked(&self.shared.post) = (status, body.to_owned());
    }

    /// Resolves once a read of a stalled thread has arrived.
    pub async fn stalled_read(&self) {
        self.shared.stalled.notified().await;
    }

    /// How many thread reads have arrived.
    #[must_use]
    pub fn reads(&self) -> usize {
        self.shared.reads.load(Ordering::SeqCst)
    }

    /// Every request received so far, on either path, in arrival order.
    #[must_use]
    pub fn requests(&self) -> Vec<Request> {
        locked(&self.shared.requests).clone()
    }

    fn script(&self, thread_ts: &str, reply: Reply) {
        locked(&self.shared.by_thread).insert(thread_ts.to_owned(), reply);
    }
}

impl Drop for FakeSlack {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// A lock no handler holds across an await, so a poisoned one only means a
/// test already panicked; its data is still what that test left.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Records a thread read and answers it from whichever script this fake has.
async fn read(shared: &Shared, headers: &HeaderMap, fields: HashMap<String, String>) -> Response {
    shared.reads.fetch_add(1, Ordering::SeqCst);
    let thread = fields.get(FIELD_TS).cloned();
    record(shared, REPLIES_PATH, headers, fields, Value::Null);
    // `None` scripts by thread; `Some(None)` is an ordered script run out.
    let ordered = locked(&shared.in_order).as_mut().map(VecDeque::pop_front);
    let reply = match ordered {
        Some(next) => next,
        None => Some(
            thread
                .and_then(|ts| locked(&shared.by_thread).get(&ts).cloned())
                .unwrap_or_else(|| Reply::answers(StatusCode::OK.as_u16(), EMPTY_THREAD)),
        ),
    };
    match reply {
        Some(Reply::Answers(status, body)) => json(status, body),
        Some(Reply::Stalls) => {
            shared.stalled.notify_one();
            tokio::time::sleep(READ_DEADLINE * 4).await;
            StatusCode::GATEWAY_TIMEOUT.into_response()
        }
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// Records a post and answers what the suite set.
fn posted(shared: &Shared, headers: &HeaderMap, sent: &str) -> Response {
    let body = serde_json::from_str::<Value>(sent).unwrap_or(Value::Null);
    let fields = body
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, value)| value.as_str().map(|text| (name.clone(), text.to_owned())))
        .collect();
    record(shared, POST_MESSAGE_PATH, headers, fields, body);
    let (status, body) = locked(&shared.post).clone();
    json(status, body)
}

fn record(
    shared: &Shared,
    path: &'static str,
    headers: &HeaderMap,
    fields: HashMap<String, String>,
    body: Value,
) {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    locked(&shared.requests).push(Request {
        path,
        authorization,
        fields,
        body,
    });
}

fn json(status: u16, body: String) -> Response {
    let status = StatusCode::from_u16(status).expect("a scripted status is valid");
    (status, [(CONTENT_TYPE, MEDIA_JSON)], body).into_response()
}
