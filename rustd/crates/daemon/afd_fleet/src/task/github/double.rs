//! A scripted TLS origin for the fixed `api.github.com` host: each connection
//! reads one request and answers the next scripted reply, then closes.

#![expect(
    clippy::expect_used,
    reason = "a fixture that cannot bind, sign or hand shake fails the test loudly"
)]

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use afd_credential::credential::outcome::Minted;
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use zeroize::Zeroizing;

use super::http::Session;

const HOST: &str = "api.github.com";

/// The installation every fixture session and target names.
pub(super) const INSTALLATION: u64 = 7;

/// One scripted answer: a status, extra headers and a body.
pub(super) struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl Reply {
    /// `200 OK` with `value` as its JSON body.
    pub(super) fn json(value: &serde_json::Value) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: serde_json::to_vec(value).expect("fixture JSON"),
        }
    }

    /// `status` with an empty JSON object as its body.
    pub(super) fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: b"{}".to_vec(),
        }
    }

    /// `200 OK` with `body` exactly as given.
    pub(super) const fn raw(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body,
        }
    }

    /// This reply with one more header.
    pub(super) fn header(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }
}

/// The listener, a client pinned to it, and every request line it read.
pub(super) struct Origin {
    pub(super) client: reqwest::Client,
    requests: Arc<Mutex<Vec<String>>>,
    worker: JoinHandle<()>,
}

impl Origin {
    /// Answers `replies` in order, one per connection; a request past the
    /// last reply finds the listener closed.
    pub(super) async fn start(replies: Vec<Reply>) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("private GitHub listener");
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
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let worker = tokio::spawn(async move {
            for reply in replies {
                let (connection, _peer) = socket.accept().await.expect("client connects");
                let mut stream = tls.accept(connection).await.expect("TLS handshake");
                let line = read(&mut stream).await;
                seen.lock().expect("request log").push(line);
                respond(&mut stream, reply).await;
            }
        });
        Self {
            client,
            requests,
            worker,
        }
    }

    /// Each request as `METHOD target`, in arrival order.
    pub(super) fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("request log").clone()
    }

    /// A session over this origin's client holding a token that lasts an hour.
    pub(super) fn session(&self, read_only: bool) -> Session<'_> {
        Session::new(&self.client, credential(), INSTALLATION, read_only)
    }
}

impl Drop for Origin {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

/// A token that expires an hour from now.
pub(super) fn credential() -> Minted {
    Minted {
        token: Zeroizing::new("native-test-read-token".to_owned()),
        expires_at: af_core::clock::after(af_core::clock::now(), Duration::from_secs(3600)),
        rotated_refresh_token: None,
    }
}

/// The request line's method and target, after the whole request is read.
async fn read(stream: &mut TlsStream<tokio::net::TcpStream>) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let end = loop {
        let count = stream.read(&mut chunk).await.expect("request bytes");
        assert!(count > 0, "complete request headers");
        bytes.extend_from_slice(chunk.get(..count).expect("read stays within chunk"));
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let head = std::str::from_utf8(bytes.get(..end).expect("located header delimiter"))
        .expect("HTTP header")
        .to_owned();
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _value)| name.eq_ignore_ascii_case("content-length"))
        .map_or(0, |(_name, value)| {
            value.trim().parse::<usize>().expect("body size")
        });
    while bytes.len() - end < length {
        let count = stream.read(&mut chunk).await.expect("body bytes");
        assert!(count > 0, "complete request body");
        bytes.extend_from_slice(chunk.get(..count).expect("read stays within chunk"));
    }
    let mut first = head
        .lines()
        .next()
        .expect("request line")
        .split_whitespace();
    let method = first.next().expect("request method");
    let target = first.next().expect("request target");
    format!("{method} {target}")
}

/// Writes `reply`; a client that stopped reading early only ends the write.
async fn respond(stream: &mut TlsStream<tokio::net::TcpStream>, reply: Reply) {
    let mut head = format!(
        "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (name, value) in &reply.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    if stream.write_all(head.as_bytes()).await.is_err()
        || stream.write_all(&reply.body).await.is_err()
    {
        return;
    }
    let _closed = stream.shutdown().await;
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
