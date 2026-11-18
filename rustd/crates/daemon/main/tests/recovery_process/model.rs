//! A controlled `OpenAI` stream on pinned TLS; every accepted request is observed by the parent.

use super::Checked;
use af_stream::test_util::IdentityFixture;
use axum::{
    Json, Router, extract::State, http, response::IntoResponse, routing::post, serve::Listener,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject as _};
use serde_json::{Value, json};
use std::{io, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

const HOST: &str = af_providers::testing::tls::HOST;

pub(super) struct Asked {
    pub body: Value,
    answer: oneshot::Sender<String>,
}

impl Asked {
    pub(super) fn answer(self, text: &str) -> Checked {
        self.answer
            .send(text.to_owned())
            .map_err(|_answer| std::io::Error::other("model request dropped"))?;
        Ok(())
    }
}

pub(super) struct Model {
    identity: IdentityFixture,
    pub address: SocketAddr,
    requests: mpsc::Receiver<(jiff::Timestamp, Asked)>,
    stop: CancellationToken,
    serving: AbortOnDropHandle<io::Result<()>>,
}

impl Model {
    pub(super) async fn start() -> Checked<Self> {
        let identity = IdentityFixture::new(vec![HOST.to_owned()])?;
        // The runner transport caps native lanes per connection. A model
        // endpoint must be able to answer another wave on the same socket.
        let listener = ProviderListener::bind(&identity).await?;
        let address = listener.local_addr()?;
        let (sender, requests) = mpsc::channel(8);
        let router = Router::new()
            .route("/v1/chat/completions", post(answer))
            .with_state(sender);
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let serving = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
        });
        Ok(Self {
            identity,
            address,
            requests,
            stop,
            serving: AbortOnDropHandle::new(serving),
        })
    }

    pub(super) fn base(&self) -> String {
        format!("https://{HOST}:{}/v1", self.address.port())
    }

    pub(super) fn root(&self) -> PathBuf {
        self.identity.root()
    }

    pub(super) async fn next(&mut self) -> Checked<Asked> {
        Ok(
            tokio::time::timeout(Duration::from_secs(75), self.next_timed())
                .await??
                .1,
        )
    }

    pub(super) async fn next_timed(&mut self) -> Checked<(jiff::Timestamp, Asked)> {
        self.requests
            .recv()
            .await
            .ok_or_else(|| std::io::Error::other("model listener stopped").into())
    }

    pub(super) async fn finish(self) -> Checked {
        self.stop.cancel();
        drop(self.requests);
        self.serving.await??;
        Ok(())
    }
}

struct ProviderListener {
    socket: TcpListener,
    acceptor: TlsAcceptor,
}

impl ProviderListener {
    async fn bind(identity: &IdentityFixture) -> Checked<Self> {
        let chain =
            CertificateDer::pem_file_iter(identity.chain())?.collect::<Result<Vec<_>, _>>()?;
        let key = PrivateKeyDer::from_pem_file(identity.key())?;
        let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(chain, key)?;
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(Self {
            socket: TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?,
            acceptor: TlsAcceptor::from(Arc::new(config)),
        })
    }
}

impl Listener for ProviderListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (socket, peer) = Listener::accept(&mut self.socket).await;
            if let Ok(Ok(stream)) =
                tokio::time::timeout(Duration::from_secs(5), self.acceptor.accept(socket)).await
            {
                return (stream, peer);
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

async fn answer(
    State(sender): State<mpsc::Sender<(jiff::Timestamp, Asked)>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let (answer, received) = oneshot::channel();
    if sender
        .send((af_core::clock::now(), Asked { body, answer }))
        .await
        .is_err()
    {
        return http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Ok(text) = received.await else {
        return http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let id = afd_db::test_util::mint_id();
    let chunk = json!({"id":id,"object":"chat.completion.chunk","created":0,"model":"fixture",
        "choices":[{"index":0,"delta":{"role":"assistant","content":text},"finish_reason":null}]});
    let end = json!({"id":id,"object":"chat.completion.chunk","created":0,"model":"fixture",
        "choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":321,"completion_tokens":123,"total_tokens":444}});
    (
        [(http::header::CONTENT_TYPE, "text/event-stream")],
        format!("data: {chunk}\n\ndata: {end}\n\ndata: [DONE]\n\n"),
    )
        .into_response()
}
