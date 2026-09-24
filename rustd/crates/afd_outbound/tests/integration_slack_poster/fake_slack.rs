//! A loopback Slack Web API, answering one scripted status to every post.
//!
//! On axum rather than a raw socket, so the request is read by a real HTTP
//! server and the cases assert its parsed fields — the bearer, the channel,
//! the thread — rather than searching the bytes for them. It serves only the
//! method the poster calls, so a post to any other path is a 404 and fails.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse as _;
use axum::routing::post;
use serde_json::Value;
use tokio::task::JoinHandle;

/// The one Slack method the poster calls.
const POST_MESSAGE: &str = "/chat.postMessage";
/// What Slack's answers are typed as.
const MEDIA_JSON: &str = "application/json";

/// One post as the fake received it.
#[derive(Debug, Clone)]
pub(super) struct Posted {
    /// The `Authorization` header, verbatim.
    pub(super) authorization: String,
    /// The JSON body, parsed.
    body: Value,
}

impl Posted {
    /// A string field of the posted body.
    pub(super) fn field(&self, name: &str) -> Option<&str> {
        self.body.get(name).and_then(Value::as_str)
    }
}

/// A loopback Slack answering `status` with `body` to every post.
pub(super) struct FakeSlack {
    pub(super) base: String,
    posts: Arc<Mutex<Vec<Posted>>>,
    handle: JoinHandle<()>,
}

impl FakeSlack {
    pub(super) async fn answering(status: u16, body: &'static str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("the listener is bound")
        );
        let status = StatusCode::from_u16(status).expect("a scripted status is valid");
        let posts = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&posts);
        let router = Router::new().route(
            POST_MESSAGE,
            post(move |headers: HeaderMap, sent: String| {
                recorded
                    .lock()
                    .expect("no test holds this lock across a panic")
                    .push(Posted {
                        authorization: headers
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_owned(),
                        body: serde_json::from_str(&sent).unwrap_or(Value::Null),
                    });
                async move { (status, [(CONTENT_TYPE, MEDIA_JSON)], body).into_response() }
            }),
        );
        let handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("the fake Slack serves until aborted");
        });
        Self {
            base,
            posts,
            handle,
        }
    }

    /// The post the poster sent, which it has finished sending by the time a
    /// verdict came back. Fails the case when nothing was posted, so a
    /// regression answering `Delivered` without dialling cannot pass.
    pub(super) fn received(&self) -> Posted {
        self.posts
            .lock()
            .expect("no test holds this lock across a panic")
            .first()
            .cloned()
            .expect("the poster posted to the fake Slack")
    }
}

impl Drop for FakeSlack {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
