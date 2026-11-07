//! Responses and captured requests for the routed HTTPS boundary.
#![expect(
    clippy::expect_used,
    reason = "test support: invalid scripted responses must fail loudly"
)]

use hyper::header::HeaderMap;
use hyper::{Method, StatusCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const JSON: &str = "application/json";

/// One answer to one request.
#[derive(Debug, Clone)]
pub(crate) struct Reply {
    pub(super) status: StatusCode,
    pub(super) location: Option<String>,
    pub(super) body: String,
    /// Stream the body, then fail the stream, so the connection closes
    /// mid-read after the declared length promised more.
    pub(super) cut: bool,
    pub(super) content_type: &'static str,
}

impl Reply {
    /// `body`, answered with `status` as JSON.
    pub(crate) fn json(status: u16, body: &serde_json::Value) -> Self {
        Self {
            status: StatusCode::from_u16(status).expect("a scripted status is valid"),
            location: None,
            body: body.to_string(),
            cut: false,
            content_type: JSON,
        }
    }

    /// A redirect to `location`, as GitHub answers a job's log.
    pub(crate) fn redirect(status: u16, location: &str) -> Self {
        Self {
            status: StatusCode::from_u16(status).expect("a scripted status is valid"),
            location: Some(location.to_owned()),
            body: String::new(),
            cut: false,
            content_type: JSON,
        }
    }

    /// A 200 whose body stops short of the length it declares: TLS completes,
    /// the headers arrive, and the read fails partway.
    pub(crate) fn cut(body: &str) -> Self {
        Self {
            status: StatusCode::OK,
            location: None,
            body: body.to_owned(),
            cut: true,
            content_type: JSON,
        }
    }

    /// Downloaded job text or an upstream error, without a JSON wrapper.
    pub(crate) fn text(status: u16, body: &str) -> Self {
        Self {
            status: StatusCode::from_u16(status).expect("a scripted status is valid"),
            location: None,
            body: body.to_owned(),
            cut: false,
            content_type: "text/plain",
        }
    }
}

/// What one host answers at one method and path.
#[derive(Debug)]
pub(crate) struct Route {
    pub(super) host: &'static str,
    pub(super) method: Method,
    pub(crate) path: String,
    source: ReplySource,
}

type Handler = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;
enum ReplySource {
    Sequence {
        replies: Vec<Reply>,
        next: AtomicUsize,
    },
    Stateful(Handler),
}
impl std::fmt::Debug for ReplySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Sequence { .. } => "Sequence",
            Self::Stateful(_) => "Stateful",
        })
    }
}

impl Route {
    /// `method` `path` at `host`, answered by `replies` in order.
    pub(crate) fn new(host: &'static str, method: Method, path: &str, replies: Vec<Reply>) -> Self {
        Self {
            host,
            method,
            path: path.to_owned(),
            source: ReplySource::Sequence {
                replies,
                next: AtomicUsize::new(0),
            },
        }
    }

    /// Commits fixture-owned remote state before a response reaches the wire.
    pub(crate) fn stateful(
        host: &'static str,
        method: Method,
        path: &str,
        handler: Handler,
    ) -> Self {
        Self {
            host,
            method,
            path: path.to_owned(),
            source: ReplySource::Stateful(handler),
        }
    }

    /// The next reply, the last one repeating.
    pub(super) fn reply(&self, request: &Seen) -> Option<Reply> {
        match &self.source {
            ReplySource::Sequence { replies, next } => {
                let index = next.fetch_add(1, Ordering::Relaxed);
                replies.get(index).or_else(|| replies.last()).cloned()
            }
            ReplySource::Stateful(handler) => Some(handler(request)),
        }
    }
}

/// One request the fake saw.
#[derive(Debug)]
pub(crate) struct Seen {
    pub(crate) host: String,
    pub(crate) method: Method,
    pub(crate) path: String,
    /// The query, empty when the request carried none.
    pub(crate) query: String,
    pub(crate) headers: HeaderMap,
    pub(crate) body: String,
}
