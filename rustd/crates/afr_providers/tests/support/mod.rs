//! What the provider suites share: a fake provider on a real socket, a lease
//! naming it, and the loop driving that lease.
//!
//! The fake answers each request with the next scripted reply, read through
//! an atomic cursor, and streams a reply's events one per body chunk, so the
//! transport is exercised and not only the parser (RULE STR). Every request it
//! saw comes back over a channel, and every connection it accepted is counted,
//! so a suite can tell a request that never left from one the fake could not
//! read.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support: a fixture that cannot be built is a broken test"
)]

pub(crate) mod wires;

use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afr_agent::testing::Discard;
use afr_agent::{AgentEngine as _, AgentRun, Loop, Meter, RunOutput};
use afr_egress::testing::CountingMint;
use afr_executor::Executor;
use afr_providers::{Connector, ProviderSpec, Registry, Wire};
use afr_tools::Catalog;
use afr_tools::catalog::UPDATE_PLAN;
use afr_tools::stub::Stub;
use axum::body::{Body, Bytes};
use axum::http::header::{CONTENT_TYPE, LOCATION, RETRY_AFTER};
use axum::http::{HeaderMap, Response, StatusCode, Uri};
use axum::serve::Listener;
use futures_util::stream;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The provider key every suite's lease carries.
pub(crate) const KEY: &str = "sk-test-provider-key-0123456789";
/// The static credential every suite's lease carries.
pub(crate) const TOKEN: &str = "ghp_suite_token_abcdef";
/// The lease every suite's run is.
pub(crate) const LEASE_ID: &str = "lease-1";
/// The content type a streamed turn answers with.
const EVENT_STREAM: &str = "text/event-stream";

/// One scripted answer to one request.
#[derive(Debug, Clone)]
pub(crate) enum Reply {
    /// A streamed turn, one Server-Sent Event per body chunk.
    Stream(Vec<String>),
    /// A refusal or a fault, with the wait it asks for.
    Status {
        status: u16,
        retry_after: Option<&'static str>,
    },
    /// A redirect to `location`.
    Redirect(String),
}

/// One request the fake saw.
#[derive(Debug)]
pub(crate) struct Seen {
    pub(crate) path: String,
    pub(crate) headers: HeaderMap,
    pub(crate) body: serde_json::Value,
    /// The body as it arrived, byte for byte.
    pub(crate) raw: Bytes,
}

/// The scripted replies, played in order through a lock-free cursor.
#[derive(Debug)]
struct Script {
    replies: Vec<Reply>,
    next: AtomicUsize,
}

/// A fake provider listening on a loopback port.
#[derive(Debug)]
pub(crate) struct Fake {
    pub(crate) base: String,
    seen: mpsc::UnboundedReceiver<Seen>,
    accepted: Arc<AtomicUsize>,
}

/// The fake's listener, counting every connection it accepts: a request the
/// fake could not read, such as a TLS handshake against its plain HTTP, still
/// opened one, where a request the client refused to send opened none.
#[derive(Debug)]
struct Counting {
    listener: TcpListener,
    accepted: Arc<AtomicUsize>,
}

impl Listener for Counting {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let accepted = Listener::accept(&mut self.listener).await;
        self.accepted.fetch_add(1, Ordering::Relaxed);
        accepted
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Listener::local_addr(&self.listener)
    }
}

impl Fake {
    /// Serves `replies`, one per request, in order; a request past the end is
    /// a 500.
    pub(crate) async fn serve(replies: Vec<Reply>) -> Self {
        let script = Arc::new(Script {
            replies,
            next: AtomicUsize::new(0),
        });
        let (sent, seen) = mpsc::unbounded_channel();
        let app = axum::Router::new().fallback(move |uri: Uri, headers: HeaderMap, body: Bytes| {
            let script = Arc::clone(&script);
            let sent = sent.clone();
            async move {
                let raw = body;
                let body = serde_json::from_slice(&raw).unwrap_or_default();
                let path = uri.path().to_owned();
                sent.send(Seen {
                    path,
                    headers,
                    body,
                    raw,
                })
                .expect("the suite holds the receiver");
                let index = script.next.fetch_add(1, Ordering::Relaxed);
                answer(script.replies.get(index))
            }
        });
        let listener = Counting {
            listener: TcpListener::bind("127.0.0.1:0").await.unwrap(),
            accepted: Arc::new(AtomicUsize::new(0)),
        };
        let address = listener.local_addr().unwrap();
        let accepted = Arc::clone(&listener.accepted);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            base: format!("http://{address}"),
            seen,
            accepted,
        }
    }

    /// Every request seen since the last read.
    pub(crate) fn seen(&mut self) -> Vec<Seen> {
        std::iter::from_fn(|| self.seen.try_recv().ok()).collect()
    }

    /// How many connections were opened to the fake, readable or not.
    pub(crate) fn connections(&self) -> usize {
        self.accepted.load(Ordering::Relaxed)
    }
}

/// The response one scripted reply is.
fn answer(reply: Option<&Reply>) -> Response<Body> {
    let builder = Response::builder();
    match reply {
        Some(Reply::Stream(events)) => {
            let chunks = events.clone().into_iter().map(Ok::<_, Infallible>);
            builder
                .header(CONTENT_TYPE, EVENT_STREAM)
                .body(Body::from_stream(stream::iter(chunks)))
        }
        Some(Reply::Status {
            status,
            retry_after,
        }) => {
            let builder = builder.status(StatusCode::from_u16(*status).unwrap());
            let builder = match retry_after {
                Some(wait) => builder.header(RETRY_AFTER, *wait),
                None => builder,
            };
            builder.body(Body::empty())
        }
        Some(Reply::Redirect(location)) => builder
            .status(StatusCode::TEMPORARY_REDIRECT)
            .header(LOCATION, location.as_str())
            .body(Body::empty()),
        None => builder
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty()),
    }
    .unwrap()
}

/// A lease naming `provider`, offering `tools`, carrying [`KEY`] and
/// [`TOKEN`], asking `message`.
pub(crate) fn lease(provider: &str, tools: &[&str], message: &str) -> LeasePayload<'static> {
    let request = serde_json::json!({ "message": message }).to_string();
    let document = serde_json::json!({
        "lease_id": LEASE_ID, "fencing_token": 1, "lease_expires_at": 1,
        "secret_delivery": "inline",
        "event": {"event_id": "e", "fleet_id": "f", "workspace_id": "w", "actor": "a",
            "event_type": "chat", "request_json": request, "created_at": 1},
        "policy": {"network_policy": {"allow": [], "read_only": true, "read_post_paths": []},
            "tools": tools, "secrets_map": {"github": {"token": TOKEN, "host": "api.github.com"}},
            "mintable": [], "provider": provider, "api_key": KEY, "inference_host": "",
            "base_url": null, "repository_binding": null, "http_origin_policies": [],
            "context": {"tool_window": 0, "memory_checkpoint_every": 0,
                "stage_chunk_threshold": 0.75, "model": "model-1", "context_cap_tokens": 0}},
        "instructions": "Read the run.", "bundle": null
    });
    // Leaked so the borrowed payload lives as long as the test that reads it.
    let text: &'static str = Box::leak(document.to_string().into_boxed_str());
    serde_json::from_str(text).unwrap()
}

/// The name the fake's chat wire is registered under.
pub(crate) const CHAT_PROVIDER: &str = "fake-chat";

/// The loop hosting a stub plan tool, with each wire's provider served by
/// `fake`.
pub(crate) fn engine(fake: &Fake) -> Loop {
    Loop::new(
        Catalog::new(vec![Stub::boxed(&UPDATE_PLAN)]),
        connector(fake),
    )
}

/// The connector reaching each wire's provider at `fake`.
pub(crate) fn connector(fake: &Fake) -> Connector {
    let entry = |name: &str, wire, base_url: String| ProviderSpec {
        name: name.to_owned(),
        aliases: Vec::new(),
        wire,
        base_url,
        dialect: None,
    };
    let registry = Registry::new([
        entry("anthropic", Wire::Messages, fake.base.clone()),
        entry("openai", Wire::Responses, format!("{}/v1", fake.base)),
        entry(CHAT_PROVIDER, Wire::Chat, format!("{}/v1", fake.base)),
    ])
    .unwrap();
    Connector::new(registry).unwrap()
}

/// The loop hosting `catalog`, with each wire's provider served by `fake`.
pub(crate) fn engine_hosting(fake: &Fake, catalog: Catalog) -> Loop {
    Loop::new(catalog, connector(fake))
}

/// Runs `lease` on `engine` to its end, with every frame it sent.
pub(crate) async fn run(
    engine: &Loop,
    lease: &LeasePayload<'_>,
) -> (RunOutput, Vec<ActivityFrame<'static>>) {
    run_with(engine, lease, None).await
}

/// Runs `lease` on `engine` to its end, its sandbox-side calls served by
/// `executor`, with every frame it sent.
pub(crate) async fn run_with(
    engine: &Loop,
    lease: &LeasePayload<'_>,
    executor: Option<&dyn Executor>,
) -> (RunOutput, Vec<ActivityFrame<'static>>) {
    let (sent, frames) = std::sync::mpsc::channel();
    let sink = move |frame| sent.send(frame).expect("the suite holds the receiver");
    let stop = CancellationToken::new();
    let run = AgentRun {
        lease,
        memory: afr_memory::Seed::default(),
        executor,
        mint: &CountingMint::never(),
        verbs: &afr_tools::CLOSED,
        checkpoint: &Discard,
        events: &sink,
        meter: &Meter::default(),
        stop: &stop,
    };
    let output = engine.run(run).await.unwrap();
    (output, frames.try_iter().collect())
}
