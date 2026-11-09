//! The proxy serves the standard CONNECT upgrade while native TLS and framing remain end to end.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_util::{
    sync::CancellationToken,
    task::{AbortOnDropHandle, TaskTracker},
};

pub(super) struct Proxy {
    pub(super) url: String,
    tunnels: Arc<AtomicUsize>,
    stop: CancellationToken,
    task: AbortOnDropHandle<std::io::Result<()>>,
    copies: TaskTracker,
}

impl Proxy {
    pub(super) async fn start(backend: SocketAddr) -> Self {
        let listener = TcpListener::bind("[::1]:0")
            .await
            .expect("CONNECT proxy bind");
        let address = listener.local_addr().expect("CONNECT proxy address");
        let tunnels = Arc::new(AtomicUsize::new(0));
        let stop = CancellationToken::new();
        let copies = TaskTracker::new();
        let serve = serve(
            listener,
            backend,
            Arc::clone(&tunnels),
            stop.clone(),
            copies.clone(),
        );
        let task = AbortOnDropHandle::new(tokio::spawn(serve));
        Self {
            url: format!("http://[::1]:{}", address.port()),
            tunnels,
            stop,
            task,
            copies,
        }
    }

    pub(super) fn tunnels(&self) -> usize {
        self.tunnels.load(Ordering::SeqCst)
    }

    pub(super) async fn close(mut self) {
        self.stop.cancel();
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT, async {
            (&mut self.task)
                .await
                .expect("CONNECT proxy task joins")
                .expect("CONNECT proxy I/O");
            self.copies.close();
            self.copies.wait().await;
        })
        .await
        .expect("CONNECT proxy shutdown deadline");
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn serve(
    listener: TcpListener,
    backend: SocketAddr,
    tunnels: Arc<AtomicUsize>,
    stop: CancellationToken,
    copies: TaskTracker,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => { workers.shutdown().await; return Ok(()); }
            done = workers.join_next(), if !workers.is_empty() => {
                let _closed_connection = done.ok_or_else(|| std::io::Error::other("missing CONNECT worker"))?.map_err(std::io::Error::other)?;
            }
            accepted = listener.accept() => {
                let (socket,_) = accepted?;
                let tunnels = Arc::clone(&tunnels);
                let stop = stop.clone();
                let copies = copies.clone();
                workers.spawn(async move {
                    let service = hyper::service::service_fn(move |request| {
                        tunnel(request,backend,Arc::clone(&tunnels),stop.clone(),copies.clone())
                    });
                    hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(socket),service).with_upgrades().await
                });
            }
        }
    }
}

async fn tunnel(
    mut request: Request<Incoming>,
    backend: SocketAddr,
    tunnels: Arc<AtomicUsize>,
    stop: CancellationToken,
    copies: TaskTracker,
) -> Result<Response<Body>, Infallible> {
    let authority = backend.to_string();
    if request.method() != Method::CONNECT
        || request
            .uri()
            .authority()
            .map(axum::http::uri::Authority::as_str)
            != Some(authority.as_str())
    {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::BAD_REQUEST;
        return Ok(response);
    }
    let connected = TcpStream::connect(backend).await;
    let Ok(mut server) = connected else {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::BAD_GATEWAY;
        return Ok(response);
    };
    tunnels.fetch_add(1, Ordering::SeqCst);
    let upgrade = hyper::upgrade::on(&mut request);
    copies.spawn(async move {
        if let Ok(upgraded) = upgrade.await {
            let mut client = TokioIo::new(upgraded);
            tokio::select! { biased; () = stop.cancelled() => {}, _closed = tokio::io::copy_bidirectional(&mut client,&mut server) => {} }
        }
    });
    Ok(Response::new(Body::empty()))
}
