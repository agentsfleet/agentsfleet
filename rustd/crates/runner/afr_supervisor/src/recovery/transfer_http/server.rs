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
    collections::BTreeMap,
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{Barrier, mpsc, oneshot},
    task::JoinHandle,
};
use tokio_rustls::TlsAcceptor;

const HOST: &str = "checkpoint.fixture.test";
#[derive(Clone)]
struct Reply {
    status: StatusCode,
    body: Vec<u8>,
    chunked: bool,
    headers: HeaderMap,
    payloads: BTreeMap<String, Vec<u8>>,
    barrier: Option<Arc<Barrier>>,
}
pub(in crate::recovery) struct Seen {
    pub(in crate::recovery) method: Method,
    pub(in crate::recovery) path: String,
    pub(in crate::recovery) headers: HeaderMap,
    pub(in crate::recovery) bytes: Bytes,
}
pub(crate) struct Server {
    pub(super) transfers: Transfers,
    pub(in crate::recovery) url: String,
    pub(in crate::recovery) seen: mpsc::UnboundedReceiver<Seen>,
    worker: JoinHandle<Result<()>>,
    stop: oneshot::Sender<()>,
    requests: Arc<AtomicUsize>,
    expected: usize,
}
impl Server {
    pub(in crate::recovery) async fn start(
        status: StatusCode,
        body: Vec<u8>,
        chunked: bool,
    ) -> Result<Self> {
        Self::with_headers(status, body, chunked, HeaderMap::new()).await
    }

    pub(crate) async fn with_headers(
        status: StatusCode,
        body: Vec<u8>,
        chunked: bool,
        headers: HeaderMap,
    ) -> Result<Self> {
        Self::configured(
            Reply {
                status,
                body,
                chunked,
                headers,
                payloads: BTreeMap::new(),
                barrier: None,
            },
            1,
        )
        .await
    }

    pub(in crate::recovery) async fn simultaneous(
        payloads: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self> {
        let expected = payloads.len();
        Self::configured(
            Reply {
                status: StatusCode::OK,
                body: Vec::new(),
                chunked: false,
                headers: HeaderMap::new(),
                payloads,
                barrier: Some(Arc::new(Barrier::new(expected))),
            },
            expected,
        )
        .await
    }

    async fn configured(reply: Reply, expected: usize) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (certificate, acceptor) = tls()?;
        let client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(5))
            .no_proxy()
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
            reply,
        ));
        Ok(Self {
            transfers: Transfers { client },
            url: format!("https://{HOST}/ciphertext?signature=fixture"),
            seen,
            worker,
            stop,
            requests,
            expected,
        })
    }
    pub(in crate::recovery) fn transfers(&self) -> Transfers {
        Transfers {
            client: self.transfers.client.clone(),
        }
    }

    pub(crate) fn client(&self) -> reqwest::Client {
        self.transfers.client.clone()
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.url
    }

    pub(crate) async fn finish(self) -> Result<()> {
        self.stop
            .send(())
            .map_err(|()| "fixture listener stopped early")?;
        tokio::time::timeout(Duration::from_secs(5), self.worker).await???;
        assert_eq!(
            self.requests.load(Ordering::SeqCst),
            self.expected,
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
    let mut workers = tokio::task::JoinSet::new();
    loop {
        let (socket, _) = tokio::select! {
            _ = &mut stopped => { workers.shutdown().await; return Ok(()); },
            result = workers.join_next(), if !workers.is_empty() => { result.ok_or("HTTP worker missing")???; continue; },
            accepted = listener.accept() => accepted?,
        };
        let stream = acceptor.accept(socket).await?;
        let (sender, requests, reply) = (sender.clone(), Arc::clone(&requests), reply.clone());
        workers.spawn(async move {
            let service = service_fn(move |request: hyper::Request<Incoming>| {
                requests.fetch_add(1, Ordering::SeqCst);
                let (reply, sender) = (reply.clone(), sender.clone());
                async move {
                    let status = if request.uri().path() == "/must-not-follow" {
                        StatusCode::OK
                    } else {
                        reply.status
                    };
                    let body = reply
                        .payloads
                        .get(request.uri().path())
                        .cloned()
                        .unwrap_or(reply.body);
                    if let Some(barrier) = &reply.barrier {
                        barrier.wait().await;
                    }
                    answer(request, sender, status, body, reply.chunked, reply.headers).await
                }
            });
            hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
    }
}

async fn answer(
    request: hyper::Request<Incoming>,
    sender: mpsc::UnboundedSender<Seen>,
    status: StatusCode,
    body: Vec<u8>,
    chunked: bool,
    headers: HeaderMap,
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
    response.headers_mut().extend(headers);
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
