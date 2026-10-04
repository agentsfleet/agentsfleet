//! An HTTPS fake for the upstreams a bundle names.
//!
//! One listener answers every host. A test CA signs a leaf naming them all,
//! and the runner's egress client, routed to this address, trusts that CA
//! (`afr_egress::Network::routed`), so a request leaves through the same guard
//! and the same TLS it would in production. A route is a host, a method and a
//! path, answered by its replies in order with the last repeating, so a second
//! run reads what a first one wrote. Every request is recorded with its headers
//! and body: a suite asserts what the runner sent, credentials included.

#![expect(
    clippy::expect_used,
    reason = "test support: a fake that cannot start is a broken test"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use axum::body::Body;
use futures_util::StreamExt as _;
use hyper::body::Incoming;
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderMap, LOCATION};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::TlsAcceptor;

/// The most of one request body the fake reads.
const BODY_LIMIT: usize = 4 * 1024 * 1024;
/// How long a cut body waits after its first bytes before the connection dies,
/// so the headers have been flushed and only the body read fails.
const CUT_AFTER: std::time::Duration = std::time::Duration::from_millis(200);
/// The media type every scripted body is answered as.
const JSON: &str = "application/json";

/// One answer to one request.
#[derive(Debug, Clone)]
pub(crate) struct Reply {
    status: StatusCode,
    location: Option<String>,
    body: String,
    /// Stream the body, then fail the stream, so the connection closes
    /// mid-read after the declared length promised more.
    cut: bool,
}

impl Reply {
    /// `body`, answered with `status` as JSON.
    pub(crate) fn json(status: u16, body: &serde_json::Value) -> Self {
        Self {
            status: StatusCode::from_u16(status).expect("a scripted status is valid"),
            location: None,
            body: body.to_string(),
            cut: false,
        }
    }

    /// A redirect to `location`, as GitHub answers a job's log.
    pub(crate) fn found(location: &str) -> Self {
        Self {
            status: StatusCode::FOUND,
            location: Some(location.to_owned()),
            body: String::new(),
            cut: false,
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
        }
    }
}

/// What one host answers at one method and path.
#[derive(Debug)]
pub(crate) struct Route {
    host: &'static str,
    method: Method,
    path: String,
    replies: Vec<Reply>,
    next: AtomicUsize,
}

impl Route {
    /// `method` `path` at `host`, answered by `replies` in order.
    pub(crate) fn new(host: &'static str, method: Method, path: &str, replies: Vec<Reply>) -> Self {
        Self {
            host,
            method,
            path: path.to_owned(),
            replies,
            next: AtomicUsize::new(0),
        }
    }

    /// The next reply, the last one repeating.
    fn reply(&self) -> Option<Reply> {
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        self.replies
            .get(index)
            .or_else(|| self.replies.last())
            .cloned()
    }
}

/// One request the fake saw.
#[derive(Debug)]
pub(crate) struct Seen {
    pub(crate) host: String,
    pub(crate) method: Method,
    pub(crate) path: String,
    pub(crate) headers: HeaderMap,
    pub(crate) body: String,
}

/// The fake, listening.
#[derive(Debug)]
pub(crate) struct Upstream {
    address: SocketAddr,
    root: reqwest::Certificate,
    hosts: Vec<&'static str>,
    seen: mpsc::Receiver<Seen>,
}

impl Upstream {
    /// Serves `routes` over HTTPS for every host they name.
    pub(crate) async fn serve(routes: Vec<Route>) -> Self {
        let mut hosts: Vec<&'static str> = routes.iter().map(|route| route.host).collect();
        hosts.sort_unstable();
        hosts.dedup();
        let (root, acceptor) = tls(&hosts);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let address = listener.local_addr().expect("a bound address");
        let (sent, seen) = mpsc::channel();
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            while let Ok((stream, _peer)) = listener.accept().await {
                let (acceptor, routes, sent) =
                    (acceptor.clone(), Arc::clone(&routes), sent.clone());
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(stream).await else {
                        return;
                    };
                    let service = hyper::service::service_fn(move |request| {
                        answer(Arc::clone(&routes), sent.clone(), request)
                    });
                    let _closed = auto::Builder::new(TokioExecutor::new())
                        .serve_connection(TokioIo::new(tls), service)
                        .await;
                });
            }
        });
        Self {
            address,
            root,
            hosts,
            seen,
        }
    }

    /// The runner's egress client, every host this fake answers routed here.
    pub(crate) fn network(&self) -> afr_egress::Network {
        let routes: Vec<(&str, SocketAddr)> = self
            .hosts
            .iter()
            .map(|host| (*host, self.address))
            .collect();
        afr_egress::Network::routed(&routes, self.root.clone()).expect("the routed client builds")
    }

    /// Every request seen since the last read.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.seen.try_iter().collect()
    }
}

/// The test CA, as the runner trusts it, and an acceptor presenting a leaf it
/// signed for `hosts`.
fn tls(hosts: &[&'static str]) -> (reqwest::Certificate, TlsAcceptor) {
    let mut authority = CertificateParams::new(Vec::<String>::new()).expect("CA params");
    authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    authority.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let issuer = CertifiedIssuer::self_signed(authority, KeyPair::generate().expect("a CA key"))
        .expect("the CA signs itself");
    let leaf_key = KeyPair::generate().expect("a leaf key");
    let names: Vec<String> = hosts.iter().map(|host| (*host).to_owned()).collect();
    let leaf = CertificateParams::new(names)
        .expect("leaf params")
        .signed_by(&leaf_key, &issuer)
        .expect("the CA signs the leaf");
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("the provider speaks TLS 1.2 and 1.3")
        .with_no_client_auth()
        .with_single_cert(vec![leaf.der().clone()], key)
        .expect("the leaf and its key match");
    let root = reqwest::Certificate::from_der(issuer.der()).expect("the CA is DER");
    (root, TlsAcceptor::from(Arc::new(config)))
}

/// Records `request` and answers it from `routes`, or 404 as GitHub would.
async fn answer(
    routes: Arc<Vec<Route>>,
    sent: mpsc::Sender<Seen>,
    request: Request<Incoming>,
) -> Result<Response<Body>, std::convert::Infallible> {
    let (parts, body) = request.into_parts();
    let host = (parts.headers.get(HOST))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(':').next().unwrap_or(value).to_owned())
        .unwrap_or_default();
    let path = parts.uri.path().to_owned();
    let bytes = axum::body::to_bytes(Body::new(body), BODY_LIMIT)
        .await
        .unwrap_or_default();
    let reply = routes
        .iter()
        .find(|route| route.host == host && route.method == parts.method && route.path == path)
        .and_then(Route::reply);
    // A suite that dropped its receiver asserts nothing about what was sent.
    let _unread = sent.send(Seen {
        host,
        method: parts.method,
        path,
        headers: parts.headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    });
    let reply =
        reply.unwrap_or_else(|| Reply::json(404, &serde_json::json!({"message": "Not Found"})));
    let mut response = Response::builder()
        .status(reply.status)
        .header(CONTENT_TYPE, JSON);
    if let Some(location) = &reply.location {
        response = response.header(LOCATION, location);
    }
    if reply.cut {
        // A length past the body, the body, then a stream that fails once the
        // headers are out: the client has its status and loses the rest, as
        // from an upstream dying mid-response.
        let promised = reply.body.len().saturating_mul(2);
        let head = futures_util::stream::iter([Ok(axum::body::Bytes::from(reply.body))]);
        let dies = futures_util::stream::once(async {
            tokio::time::sleep(CUT_AFTER).await;
            Err(std::io::Error::other("the fake cut this body"))
        });
        return Ok(response
            .header(CONTENT_LENGTH, promised)
            .body(Body::from_stream(head.chain(dies)))
            .expect("a cut reply builds"));
    }
    Ok(response
        .body(Body::from(reply.body))
        .expect("a scripted reply builds"))
}
