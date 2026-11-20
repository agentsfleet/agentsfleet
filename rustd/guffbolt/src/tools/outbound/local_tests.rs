//! `LocalOutbound` never reaches a private address: not by a literal, not by
//! a name that resolves to one, and not by a redirect to either.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use super::{Allowlist, LocalOutbound, REQUEST_TIMEOUT, RESPONSE_MAX_BYTES};
use crate::tools::code;
use crate::tools::outbound::testing::{Counted, FixedLookup};
use crate::tools::outbound::{
    Outbound as _, Placement, Redirects, Refused, Request, Response, client,
};

/// The public name the fake server stands in for.
const PUBLIC: &str = "public.test";

impl LocalOutbound {
    /// Reaches `host` at `address`, trusting only `root`, and looks every
    /// other name up through `lookup`.
    ///
    /// The route skips the guard for `host` alone, since reqwest answers a
    /// routed name itself, so a fake server on loopback can stand in for a
    /// public host. That is why this lives with the tests: no constructor
    /// outside them routes a name.
    fn routed(
        lookup: Arc<FixedLookup>,
        host: &str,
        address: SocketAddr,
        root: reqwest::Certificate,
    ) -> Self {
        let client = client(REQUEST_TIMEOUT, Arc::<FixedLookup>::clone(&lookup))
            .tls_certs_only([root])
            .resolve(host, address)
            .build()
            .expect("the test client builds");
        Self {
            client,
            lookup,
            allowlist: Allowlist::Any,
            secrets: BTreeMap::new(),
        }
    }
}

/// A GET of `url` that follows its redirects.
fn get(url: &str) -> Request {
    Request {
        method: "GET".to_owned(),
        url: url.to_owned(),
        headers: Vec::new(),
        body: None,
        placement: Placement::Nowhere,
        redirects: Redirects::Follow,
    }
}

/// A new authority's root, and an acceptor presenting a leaf it signed for
/// [`PUBLIC`].
fn authority() -> (reqwest::Certificate, TlsAcceptor) {
    let mut authority = CertificateParams::new(Vec::<String>::new()).expect("CA params");
    authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    authority.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let issuer = CertifiedIssuer::self_signed(authority, KeyPair::generate().expect("a CA key"))
        .expect("the CA signs itself");
    let leaf_key = KeyPair::generate().expect("a leaf key");
    let leaf = CertificateParams::new(vec![PUBLIC.to_owned()])
        .expect("leaf params")
        .signed_by(&leaf_key, &issuer)
        .expect("the CA signs the leaf");
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("the provider speaks TLS 1.2 and 1.3")
    .with_no_client_auth()
    .with_single_cert(vec![leaf.der().clone()], key)
    .expect("the leaf and its key match");
    let root = reqwest::Certificate::from_der(issuer.der()).expect("the CA is DER");
    (root, TlsAcceptor::from(Arc::new(config)))
}

/// An HTTPS server for [`PUBLIC`] on loopback, answering each path from
/// `answers` and recording each request line it reads, and the root that
/// signed its certificate.
async fn fake(
    answers: &[(&str, &str)],
) -> (SocketAddr, reqwest::Certificate, Arc<Mutex<Vec<String>>>) {
    let (root, acceptor) = authority();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback takes a listener");
    let address = listener
        .local_addr()
        .expect("a bound listener has an address");
    let answers: BTreeMap<String, String> = answers
        .iter()
        .map(|(path, answer)| ((*path).to_owned(), (*answer).to_owned()))
        .collect();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((socket, _peer)) = listener.accept().await {
            if let Ok(stream) = acceptor.accept(socket).await {
                answer(stream, &answers, &recorded).await;
            }
        }
    });
    (address, root, seen)
}

/// Reads one request's head from `stream`, records its method and path in
/// `recorded`, and writes the answer `answers` holds for the path.
async fn answer(
    mut stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    answers: &BTreeMap<String, String>,
    recorded: &Mutex<Vec<String>>,
) {
    let mut head = Vec::new();
    let mut buffer = [0; 1024];
    while !head.windows(4).any(|end| end == b"\r\n\r\n") {
        match stream.read(&mut buffer).await {
            Ok(read @ 1..) => head.extend_from_slice(buffer.get(..read).unwrap_or_default()),
            _closed => break,
        }
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let line = text.lines().next().unwrap_or_default();
    let mut parts = line.split(' ');
    let (method, path) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    recorded
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(format!("{method} {path}"));
    let reply = answers.get(path).cloned().unwrap_or_default();
    let _written = stream.write_all(reply.as_bytes()).await;
    let _closed = stream.shutdown().await;
}

/// A redirect to `location`, as the fake writes it.
fn redirect(location: &str) -> String {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

/// What a sent request answered, or why it was refused.
async fn sent(outbound: &LocalOutbound, url: &str) -> Result<Response, Refused> {
    outbound.send("http_request", get(url)).await
}

fn requests(seen: &Mutex<Vec<String>>) -> Vec<String> {
    seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

#[tokio::test]
async fn local_outbound_refuses_a_name_resolving_to_loopback() {
    let inside = Counted::listen().await;
    let lookup = Arc::new(FixedLookup::default().answering("loopback.test", &[inside.address]));
    let outbound = LocalOutbound::resolving(Allowlist::Any, Arc::<FixedLookup>::clone(&lookup))
        .expect("the client builds");

    let refused = sent(&outbound, "https://loopback.test/").await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "loopback.test resolves to a private, loopback or reserved address",
        ))
    );
    assert_eq!(lookup.asked(), ["loopback.test"]);
    assert_eq!(inside.connections(), 0, "no connection was opened");
}

#[tokio::test]
async fn local_outbound_refuses_a_name_whose_system_answer_is_loopback() {
    let inside = Counted::listen().await;
    let outbound = LocalOutbound::new(Allowlist::Any).expect("the client builds");
    let url = format!("https://localhost:{}/", inside.address.port());

    let refused = sent(&outbound, &url).await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "localhost resolves to a private, loopback or reserved address",
        ))
    );
    assert_eq!(inside.connections(), 0, "no connection was opened");
}

#[tokio::test]
async fn local_outbound_refuses_a_private_literal_before_any_lookup() {
    let lookup = Arc::new(FixedLookup::default());
    let listed = Allowlist::Hosts(vec!["10.0.0.1".to_owned(), "[::1]".to_owned()]);
    let outbound = LocalOutbound::resolving(listed, Arc::<FixedLookup>::clone(&lookup))
        .expect("the client builds");

    for (url, host) in [
        ("https://10.0.0.1/", "10.0.0.1"),
        ("https://[::1]/", "[::1]"),
        ("https://[fd00::1]/", "[fd00::1]"),
        (
            "https://169.254.169.254/latest/meta-data",
            "169.254.169.254",
        ),
    ] {
        assert_eq!(
            sent(&outbound, url).await,
            Err(Refused::new(
                code::EGRESS_DENIED,
                format!("{host} is a private, loopback or reserved address"),
            )),
            "{url}"
        );
    }
    assert_eq!(lookup.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn local_outbound_refuses_a_redirect_to_a_private_address() {
    let to_private = redirect("https://10.0.0.1/");
    let (address, root, seen) = fake(&[("/start", to_private.as_str())]).await;
    let lookup = Arc::new(FixedLookup::default());
    let outbound = LocalOutbound::routed(Arc::clone(&lookup), PUBLIC, address, root);

    let refused = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "10.0.0.1 is a private, loopback or reserved address",
        ))
    );
    assert_eq!(requests(&seen), ["GET /start"]);
    assert_eq!(
        lookup.asked(),
        Vec::<String>::new(),
        "the literal was never looked up"
    );
}

#[tokio::test]
async fn local_outbound_refuses_a_redirect_to_a_name_resolving_privately() {
    let to_private = redirect("https://private.test/");
    let (address, root, seen) = fake(&[("/start", to_private.as_str())]).await;
    let inside = Counted::listen().await;
    let lookup = Arc::new(FixedLookup::default().answering("private.test", &[inside.address]));
    let outbound = LocalOutbound::routed(Arc::clone(&lookup), PUBLIC, address, root);

    let refused = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "private.test resolves to a private, loopback or reserved address",
        ))
    );
    assert_eq!(requests(&seen), ["GET /start"]);
    assert_eq!(lookup.asked(), ["private.test"]);
    assert_eq!(
        inside.connections(),
        0,
        "the private host got no connection"
    );
}

#[tokio::test]
async fn local_outbound_follows_a_redirect_to_a_public_host() {
    let arrived = "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\narrived";
    let onward = redirect("https://public.test/next");
    let (address, root, seen) = fake(&[("/start", onward.as_str()), ("/next", arrived)]).await;
    let outbound = LocalOutbound::routed(Arc::new(FixedLookup::default()), PUBLIC, address, root);

    let answered = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        answered,
        Ok(Response {
            status: 200,
            body: "arrived".to_owned(),
            followed_redirects: 1,
            ..Response::default()
        })
    );
    assert_eq!(requests(&seen), ["GET /start", "GET /next"]);
}

#[tokio::test]
async fn local_outbound_refuses_a_redirect_to_a_location_it_cannot_read() {
    let nowhere = redirect("https://");
    let (address, root, seen) = fake(&[("/start", nowhere.as_str())]).await;
    let outbound = LocalOutbound::routed(Arc::new(FixedLookup::default()), PUBLIC, address, root);

    let refused = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        refused,
        Err(Refused::new(code::INVALID_ARGUMENTS, "empty host"))
    );
    assert_eq!(requests(&seen), ["GET /start"]);
}

/// A body past the hook's limit is cut there, and the answer's media type
/// comes back with it.
#[tokio::test]
async fn local_outbound_cuts_a_body_at_its_limit_and_reads_the_media_type() {
    let body = "a".repeat(RESPONSE_MAX_BYTES + 1);
    let large = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (address, root, _seen) = fake(&[("/large", large.as_str())]).await;
    let outbound = LocalOutbound::routed(Arc::new(FixedLookup::default()), PUBLIC, address, root);

    let answered = sent(&outbound, "https://public.test/large")
        .await
        .expect("the server answers");

    assert_eq!(answered.content_type.as_deref(), Some("text/plain"));
    assert!(answered.truncated, "the body ran past the limit");
    assert_eq!(answered.body.len(), RESPONSE_MAX_BYTES);
}

/// A host that takes the connection and drops it unanswered reads as
/// unreachable.
#[tokio::test]
async fn local_outbound_reads_a_dropped_connection_as_unreachable() {
    let dropping = Counted::listen().await;
    let (root, _acceptor) = authority();
    let outbound = LocalOutbound::routed(
        Arc::new(FixedLookup::default()),
        PUBLIC,
        dropping.address,
        root,
    );

    let refused = sent(&outbound, "https://public.test/")
        .await
        .expect_err("nothing answered");

    assert_eq!(
        refused.code,
        code::UPSTREAM_UNREACHABLE,
        "{}",
        refused.detail
    );
    assert!(dropping.connections() >= 1, "the host was reached");
}
