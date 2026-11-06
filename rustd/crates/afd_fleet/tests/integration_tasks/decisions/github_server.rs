//! Scripted real TLS transport for the fixed GitHub origin; no production URL override.
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
mod exchange;

const HOST: &str = "api.github.com";

pub(super) struct Server {
    pub client: reqwest::Client,
    replies: Arc<Mutex<VecDeque<Value>>>,
    exchange: Arc<Mutex<Option<exchange::Exchange>>>,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}
impl Server {
    pub(super) async fn start(replies: Vec<Value>) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("private Github listener");
        let address = socket.local_addr().expect("listener address");
        let (root, tls) = tls();
        #[expect(
            clippy::disallowed_methods,
            reason = "fixture pins a generated CA hostname to its own ephemeral listener"
        )]
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(5))
            .add_root_certificate(root)
            .resolve(HOST, address)
            .build()
            .expect("pinned HTTPS client");
        let replies = Arc::new(Mutex::new(VecDeque::from(replies)));
        let owned = replies.clone();
        let exchange = Arc::new(Mutex::new(None));
        let app = exchange.clone();
        let (stop, mut stopped) = oneshot::channel();
        let worker = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    () = async { let _ = (&mut stopped).await; } => break,
                    accepted = socket.accept() => accepted,
                };
                let (socket, _) = accepted.expect("native reader connects");
                let mut stream = tls.accept(socket).await.expect("verified TLS handshake");
                respond(&mut stream, &owned, &app).await;
            }
        });
        Self {
            client,
            replies,
            exchange,
            stop: Some(stop),
            worker: Some(worker),
        }
    }
    pub(super) fn app(&self, installation: u32, public_key: &str, write: bool) {
        let mut exchange = self.exchange.lock().expect("App exchange configuration");
        assert!(exchange.is_none(), "one workspace installation per server");
        *exchange = Some(exchange::Exchange::new(installation, public_key, write));
    }
    pub(super) fn push(&self, reply: Value) {
        self.replies.lock().expect("replies").push_back(reply);
    }
    pub(super) async fn finish(mut self) {
        self.stop
            .take()
            .expect("stop handle")
            .send(())
            .expect("worker alive");
        tokio::time::timeout(Duration::from_secs(5), self.worker.take().expect("worker"))
            .await
            .expect("bounded shutdown")
            .expect("server assertions");
        if let Some(exchange) = &*self.exchange.lock().expect("App exchange") {
            exchange.finish();
        }
        assert!(
            self.replies.lock().expect("replies").is_empty(),
            "all evidence requests reached transport"
        );
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}
async fn respond(
    stream: &mut TlsStream<tokio::net::TcpStream>,
    replies: &Mutex<VecDeque<Value>>,
    exchange: &Mutex<Option<exchange::Exchange>>,
) {
    let request = read(stream).await;
    let app = exchange
        .lock()
        .expect("App API state")
        .as_mut()
        .and_then(|app| app.respond(&request));
    let reply = app.unwrap_or_else(|| evidence(&request, replies));
    let body = serde_json::to_vec(&reply).expect("response");
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await.expect("headers");
    stream.write_all(&body).await.expect("body");
    stream.shutdown().await.expect("TLS close");
}

fn evidence(request: &Request, replies: &Mutex<VecDeque<Value>>) -> Value {
    let reply = replies
        .lock()
        .expect("replies")
        .pop_front()
        .expect("no extra network request");
    let reply = if let Some(expected) = reply.get("fixture_request") {
        assert_eq!(request.method, expected["method"].as_str().expect("method"));
        assert_eq!(request.path, expected["path"].as_str().expect("path"));
        assert_eq!(request.authorization, "Bearer native-test-read-token");
        if let Some(body) = expected.get("body") {
            assert_eq!(
                &serde_json::from_slice::<Value>(&request.body).expect("request JSON"),
                body
            );
        }
        if expected.get("publication").is_some() {
            publication_request(request);
        }
        reply["response"].clone()
    } else {
        history_request(request);
        reply
    };
    reply
}

fn publication_request(request: &Request) {
    let body: Value = serde_json::from_slice(&request.body).expect("mutation body");
    assert!(
        body["query"]
            .as_str()
            .expect("mutation")
            .starts_with("mutation(")
    );
    let input = &body["variables"]["input"];
    assert_eq!(input["expectedHeadOid"], "0".repeat(40));
    assert_eq!(
        input["branch"],
        json!({"repositoryNameWithOwner":"example/linkwarden","branchName":"repair"})
    );
    assert_eq!(
        input["fileChanges"],
        json!({"additions":[{"path":"README.md","contents":"Zml4"}],"deletions":[]})
    );
    let identity = input["clientMutationId"]
        .as_str()
        .expect("native receipt identity");
    afd_core::id::Uuid7::parse(identity).expect("native UUID7");
    assert_eq!(
        input["message"]["body"],
        format!("agentsfleet-effect:{identity}")
    );
}

fn history_request(request: &Request) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/graphql");
    assert_eq!(request.authorization, "Bearer native-test-read-token");
    let body: Value = serde_json::from_slice(&request.body).expect("native GraphQL request");
    assert_eq!(
        body["variables"],
        json!({"owner":"example","name":"linkwarden","branch":"refs/heads/repair"})
    );
    let query = body["query"].as_str().expect("query");
    assert!(
        query.starts_with("query("),
        "reconciliation must not issue mutation"
    );
    assert!(query.contains("history(first:50)"));
}

struct Request {
    method: String,
    path: String,
    authorization: String,
    body: Vec<u8>,
}
async fn read(stream: &mut TlsStream<tokio::net::TcpStream>) -> Request {
    let mut bytes = Vec::new();
    let (end, length, method, path, authorization) = loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.expect("request bytes");
        assert!(count > 0, "complete request headers");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() < 32 * 1024, "fixture request bounded");
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let (length, method, path, authorization) = headers(&bytes[..end]);
            break (end + 4, length, method, path, authorization);
        }
    };
    while bytes.len() - end < length {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.expect("body bytes");
        assert!(count > 0, "complete request body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    Request {
        method,
        path,
        authorization,
        body: bytes[end..end + length].to_vec(),
    }
}

fn headers(bytes: &[u8]) -> (usize, String, String, String) {
    let header = std::str::from_utf8(bytes).expect("HTTP header");
    let mut lines = header.lines();
    let first = lines
        .next()
        .expect("request line")
        .split_whitespace()
        .collect::<Vec<_>>();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let length = headers
        .get("content-length")
        .map_or(0, |value| value.parse::<usize>().expect("body size"));
    assert!(length < 8192, "small real transport fixture");
    (
        length,
        first[0].to_owned(),
        first[1].to_owned(),
        headers
            .get("authorization")
            .cloned()
            .expect("native bearer authorization"),
    )
}

fn tls() -> (reqwest::Certificate, TlsAcceptor) {
    let mut authority = CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
    authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    authority.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let issuer = CertifiedIssuer::self_signed(authority, KeyPair::generate().expect("CA key"))
        .expect("CA certificate");
    let leaf_key = KeyPair::generate().expect("leaf key");
    let leaf = CertificateParams::new(vec![HOST.to_owned()])
        .expect("host parameters")
        .signed_by(&leaf_key, &issuer)
        .expect("leaf certificate");
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("TLS versions")
    .with_no_client_auth()
    .with_single_cert(
        vec![leaf.der().clone()],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
    )
    .expect("TLS server");
    (
        reqwest::Certificate::from_der(issuer.der()).expect("CA trust"),
        TlsAcceptor::from(Arc::new(config)),
    )
}
