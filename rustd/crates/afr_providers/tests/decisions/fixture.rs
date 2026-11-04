//! Shared external I/O fixture: atomics and channels, with owned shutdown.

use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use afd_crypto::secret::SecretString;
use afd_wire::decision::{DecisionApi, DecisionCapabilities, DecisionProfile, DecisionRequest};
use axum::{
    Router,
    body::{Body, Bytes},
    http::{HeaderMap, Response, Uri},
    serve::Listener,
};
use futures_util::stream;
use garde::{Unvalidated, Valid};
use serde_json::{Value, json};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinHandle,
};
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(super) enum Reply {
    Json(Value),
    Status(u16, Option<&'static str>),
    Bytes(Vec<u8>),
    Chunks(Vec<Vec<u8>>),
    PrivateFailure(String),
    Withheld,
    Broken,
    Redirect(String),
}

pub(super) struct Seen {
    pub path: String,
    pub headers: HeaderMap,
    pub body: Value,
}

pub(super) struct Server {
    pub base: String,
    pub client: reqwest::Client,
    seen: mpsc::UnboundedReceiver<Seen>,
    connections: Arc<AtomicUsize>,
    stop: CancellationToken,
    worker: JoinHandle<()>,
}

struct Script {
    replies: Vec<Reply>,
    next: AtomicUsize,
}
struct Tls {
    socket: TcpListener,
    acceptor: TlsAcceptor,
    connections: Arc<AtomicUsize>,
}

impl Listener for Tls {
    type Io = tokio_rustls::server::TlsStream<TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (socket, address) = self.socket.accept().await.expect("fixture accepts TCP");
            self.connections.fetch_add(1, Ordering::Relaxed);
            if let Ok(stream) = self.acceptor.accept(socket).await {
                return (stream, address);
            }
        }
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

impl Server {
    pub(super) async fn start(replies: Vec<Reply>) -> Self {
        let script = Arc::new(Script {
            replies,
            next: AtomicUsize::new(0),
        });
        let (sent, seen) = mpsc::unbounded_channel();
        let stop = CancellationToken::new();
        let response_stop = stop.clone();
        let app = Router::new().fallback(move |uri: Uri, headers: HeaderMap, bytes: Bytes| {
            let script = Arc::clone(&script);
            let sent = sent.clone();
            let response_stop = response_stop.clone();
            async move {
                sent.send(Seen {
                    path: uri.path().to_owned(),
                    headers,
                    body: serde_json::from_slice(&bytes).expect("client sends JSON"),
                })
                .expect("scenario owns request receiver");
                respond(
                    script
                        .replies
                        .get(script.next.fetch_add(1, Ordering::Relaxed)),
                    response_stop,
                )
            }
        });
        let (root, tls) = super::tls::server();
        let socket = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("isolated fixture port");
        let address = socket.local_addr().expect("bound address");
        let connections = Arc::new(AtomicUsize::new(0));
        let listener = Tls {
            socket,
            acceptor: tls,
            connections: Arc::clone(&connections),
        };
        let stopped = stop.clone();
        let worker = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
                .expect("fixture server");
        });
        Self {
            base: format!("https://{}:{}", super::tls::HOST, address.port()),
            client: super::tls::client(root, address),
            seen,
            connections,
            stop,
            worker,
        }
    }

    pub(super) async fn next(&mut self) -> Seen {
        tokio::time::timeout(Duration::from_secs(3), self.seen.recv())
            .await
            .expect("request arrives within fixture budget")
            .expect("server remains owned")
    }
    pub(super) fn unconsumed(&mut self) -> usize {
        std::iter::from_fn(|| self.seen.try_recv().ok()).count()
    }
    pub(super) fn connections(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    pub(super) async fn shutdown(&mut self) {
        self.stop.cancel();
        if let Ok(joined) = tokio::time::timeout(Duration::from_secs(1), &mut self.worker).await {
            joined.expect("fixture server did not panic");
        } else {
            self.worker.abort();
            let stopped = tokio::time::timeout(Duration::from_secs(1), &mut self.worker)
                .await
                .expect("aborted fixture terminates");
            assert!(stopped.is_err_and(|error| error.is_cancelled()));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.worker.abort();
    }
}

fn respond(reply: Option<&Reply>, stop: CancellationToken) -> Response<Body> {
    let builder = Response::builder();
    let response = match reply {
        Some(Reply::Json(value)) => builder
            .header("content-type", "application/json")
            .body(Body::from(value.to_string())),
        Some(Reply::Status(status, retry)) => {
            let builder = builder.status(*status);
            let builder = match retry {
                Some(value) => builder.header("retry-after", *value),
                None => builder,
            };
            builder.body(Body::empty())
        }
        Some(Reply::Bytes(bytes)) => builder.body(Body::from(bytes.clone())),
        Some(Reply::Chunks(chunks)) => builder.body(Body::from_stream(stream::iter(
            chunks
                .clone()
                .into_iter()
                .map(|chunk| Ok::<Bytes, Infallible>(Bytes::from(chunk))),
        ))),
        Some(Reply::PrivateFailure(text)) => builder.status(500).body(Body::from(text.clone())),
        Some(Reply::Withheld) => builder.body(Body::from_stream(stream::once(async move {
            stop.cancelled().await;
            Ok::<Bytes, Infallible>(Bytes::new())
        }))),
        Some(Reply::Broken) => builder.body(Body::from_stream(stream::once(async {
            Err::<Bytes, _>(io::Error::from(io::ErrorKind::ConnectionReset))
        }))),
        Some(Reply::Redirect(location)) => builder
            .status(307)
            .header("location", location)
            .body(Body::empty()),
        None => builder.status(500).body(Body::empty()),
    };
    response.expect("fixture response")
}

pub(super) fn profile(server: &Server, api: DecisionApi) -> Valid<DecisionProfile> {
    let path = match api {
        DecisionApi::Typesafe => "/v1/systemone",
        DecisionApi::Openai | DecisionApi::Perplexity => "/v1/decisions",
        DecisionApi::Cloudflare => "/client/v4/accounts/test-account/ai/run/@cf/cloudflare/clef",
    };
    Unvalidated::new(DecisionProfile {
        model_id: "019ed085-1300-7000-8000-000000000001".to_owned(),
        revision: "reviewed-profile-1".to_owned(),
        api,
        endpoint: format!("{}{path}", server.base),
        model: "requested-model".to_owned(),
        accepted_models: vec!["evaluated-model-1".to_owned()],
        capabilities: DecisionCapabilities {
            noul: true,
            choice: true,
            score: true,
        },
        input_max_bytes: 24576,
    })
    .validate()
    .expect("reviewed bounded profile")
}

pub(super) fn request() -> Valid<DecisionRequest> {
    Unvalidated::new(serde_json::from_value::<DecisionRequest>(json!({
        "call_id":"019ed085-1300-7000-8000-000000000002", "purpose":"goal",
        "state":{"accepted_request":"Repair the importer", "head":"head-b", "checks":{"unit":"passed", "integration":"pending"}},
        "questions":[
            {"name":"matches", "question":{"type":"noul", "instructions":"Does the repair meet the accepted scope?", "criteria":{"true":"The requested defect is fixed.", "false":"The defect remains or scope changed."}}},
            {"name":"next", "question":{"type":"choice", "instructions":"Choose the next action.", "options":[
                {"value":"WAIT", "description":"A required check is pending."}, {"value":"DONE", "description":"Every required check passed."}]}},
            {"name":"risk", "question":{"type":"score", "instructions":"Rate the patch risk.", "levels":["Low", "High"]}}
        ]
    })).expect("request document")).validate().expect("bounded native request")
}

pub(super) fn credential() -> SecretString {
    SecretString::new(["decision", "fixture", "credential"].join("-"))
}

pub(super) fn native_reply() -> Value {
    json!({"model":"evaluated-model-1", "usage":{"input_tokens":100, "output_tokens":3}, "answers":{
        "matches":{"type":"noul", "noul":0.99},
        "next":{"type":"choice", "choice":"WAIT", "confidence":0.96, "probabilities":{"WAIT":0.98, "DONE":0.02}},
        "risk":{"type":"score", "score":0.2, "confidence":0.8, "probabilities":{"0":0.8, "1":0.2}, "legend":{"0":"Low", "1":"High"}}
    }})
}

pub(super) fn openai_reply() -> Value {
    json!({
        "model":"evaluated-model-1", "usage":{"input_tokens":100, "output_tokens":3, "input_tokens_details":{"cached_tokens":20, "cache_write_tokens":10}},
        "answers":[
            {"name":"risk", "type":"score", "score":0.2, "confidence":0.8, "probabilities":[{"value":0,"label":"0","probability":0.8},{"value":1,"label":"1","probability":0.2}]},
            {"name":"matches", "type":"predicate", "probability":0.99},
            {"name":"next", "type":"choice", "choice":"WAIT", "confidence":0.96, "probabilities":[{"value":"WAIT","probability":0.98},{"value":"DONE","probability":0.02}]}
        ]
    })
}

pub(super) fn envelope(api: DecisionApi, document: Value) -> Value {
    if api == DecisionApi::Cloudflare {
        json!({"success":true,"result":document,"errors":[]})
    } else {
        document
    }
}

pub(super) fn replaced(original: &Value, path: &str, replacement: Value) -> Value {
    let mut document = original.clone();
    *document
        .pointer_mut(path)
        .expect("fault path exists in the external fixture") = replacement;
    document
}
