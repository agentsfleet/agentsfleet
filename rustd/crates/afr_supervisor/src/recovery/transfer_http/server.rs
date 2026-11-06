//! Verified HTTPS with the native HTTP parser; every request is observed on wire.
use super::{Result, Transfers};
use axum::{
    body::{Body, Bytes, to_bytes},
    http::{HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use hyper::{body::Incoming, service::service_fn};
use hyper_util::rt::TokioIo;
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_rustls::TlsAcceptor;

const HOST: &str = "checkpoint.fixture.test";
#[derive(Clone)]
struct Reply {
    status: StatusCode,
    body: Vec<u8>,
    chunked: bool,
}
pub(super) struct Seen {
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) headers: HeaderMap,
    pub(super) bytes: Bytes,
}
pub(super) struct Server {
    pub(super) transfers: Transfers,
    pub(super) url: String,
    pub(super) seen: mpsc::UnboundedReceiver<Seen>,
    worker: JoinHandle<Result<()>>,
    stop: oneshot::Sender<()>,
    requests: Arc<AtomicUsize>,
}
impl Server {
    pub(super) async fn start(status: StatusCode, body: Vec<u8>, chunked: bool) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (certificate, acceptor) = tls()?;
        let client = afd_identity::direct(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .add_root_certificate(certificate)
            .resolve(HOST, address)
            .build()?;
        let (sender, seen) = mpsc::unbounded_channel();
        let (stop, stopped) = oneshot::channel();
        let requests = Arc::new(AtomicUsize::new(0));
        let worker = tokio::spawn(serve(
            listener,
            acceptor,
            sender,
            stopped,
            Arc::clone(&requests),
            Reply {
                status,
                body,
                chunked,
            },
        ));
        Ok(Self {
            transfers: Transfers { client },
            url: format!("https://{HOST}/ciphertext?signature=fixture"),
            seen,
            worker,
            stop,
            requests,
        })
    }
    pub(super) async fn finish(self) -> Result<()> {
        self.stop
            .send(())
            .map_err(|()| "fixture listener stopped early")?;
        tokio::time::timeout(Duration::from_secs(5), self.worker).await???;
        assert_eq!(
            self.requests.load(Ordering::SeqCst),
            1,
            "no redirected or repeated physical request"
        );
        Ok(())
    }
}
async fn serve(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    sender: mpsc::UnboundedSender<Seen>,
    mut stopped: oneshot::Receiver<()>,
    requests: Arc<AtomicUsize>,
    reply: Reply,
) -> Result<()> {
    loop {
        let (socket, _) = tokio::select! {
            _ = &mut stopped => return Ok(()),
            accepted = listener.accept() => accepted?,
        };
        let stream = acceptor.accept(socket).await?;
        let (sender, requests, reply) = (sender.clone(), Arc::clone(&requests), reply.clone());
        let service = service_fn(move |request: hyper::Request<Incoming>| {
            requests.fetch_add(1, Ordering::SeqCst);
            let status = if request.uri().path() == "/must-not-follow" {
                StatusCode::OK
            } else {
                reply.status
            };
            answer(
                request,
                sender.clone(),
                status,
                reply.body.clone(),
                reply.chunked,
            )
        });
        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await?;
    }
}
async fn answer(
    request: hyper::Request<Incoming>,
    sender: mpsc::UnboundedSender<Seen>,
    status: StatusCode,
    body: Vec<u8>,
    chunked: bool,
) -> std::result::Result<Response<Body>, Infallible> {
    let (parts, incoming) = request.into_parts();
    let Ok(bytes) = to_bytes(Body::new(incoming), 4096).await else {
        return Ok(Response::new(Body::empty()));
    };
    let _sent = sender.send(Seen {
        method: parts.method,
        path: parts.uri.to_string(),
        headers: parts.headers,
        bytes,
    });
    let mut response = Response::new(if chunked {
        Body::from_stream(futures_util::stream::iter([Ok::<_, Infallible>(
            Bytes::from(body),
        )]))
    } else {
        Body::from(body)
    });
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert("connection", HeaderValue::from_static("close"));
    response
        .headers_mut()
        .insert("location", HeaderValue::from_static("/must-not-follow"));
    Ok(response)
}
fn tls() -> Result<(reqwest::Certificate, TlsAcceptor)> {
    let mut authority = CertificateParams::new(Vec::<String>::new())?;
    authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let issuer = CertifiedIssuer::self_signed(authority, KeyPair::generate()?)?;
    let key = KeyPair::generate()?;
    let leaf = CertificateParams::new(vec![HOST.to_owned()])?.signed_by(&key, &issuer)?;
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(
        vec![leaf.der().clone()],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
    )?;
    Ok((
        reqwest::Certificate::from_der(issuer.der())?,
        TlsAcceptor::from(Arc::new(config)),
    ))
}
