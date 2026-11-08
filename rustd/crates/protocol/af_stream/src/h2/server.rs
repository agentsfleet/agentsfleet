use std::{convert::Infallible, time::Duration};

use http::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tower::{Service, ServiceExt};

use crate::{
    Incoming, Outgoing, Result, Stage, Transport,
    error::{ErrorKind, within},
    server::ConnectionInfo,
    tls::Identity,
};

struct Session<S> {
    service: S,
    stop: CancellationToken,
}

impl<S> Session<S>
where
    S: Service<Request<Incoming>, Response = Response<Outgoing>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    async fn serve(self, socket: TcpStream, tls: &boring::ssl::SslAcceptor) -> Result<()> {
        socket.set_nodelay(true)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let tls = within(&self.stop, deadline, Stage::Handshake, async {
            Ok(tokio_boring::accept(tls, socket).await?)
        })
        .await?;
        if tls.ssl().selected_alpn_protocol() != Some(b"h2".as_slice()) {
            return Err(ErrorKind::Protocol.into());
        }
        let executor = super::executor::Executor::new(self.stop.clone());
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        let service = router(
            self.service,
            self.stop.clone(),
            std::sync::Arc::clone(&ready),
        );
        let connection = hyper::server::conn::http2::Builder::new(executor.clone())
            .initial_stream_window_size(crate::STREAM_WINDOW)
            .initial_connection_window_size(crate::CONNECTION_WINDOW)
            .max_concurrent_streams(crate::STREAMS)
            .max_header_list_size(crate::HEADER_BYTES)
            .max_send_buf_size(crate::CHUNK_BYTES)
            .serve_connection(TokioIo::new(tls), service);
        executor
            .run(drive(connection, ready, &self.stop, deadline))
            .await
    }
}

async fn drive(
    connection: impl Future<Output = Result<(), hyper::Error>>,
    ready: std::sync::Arc<tokio::sync::Notify>,
    stop: &CancellationToken,
    deadline: Instant,
) -> Result<()> {
    tokio::pin!(connection);
    let opening = within(stop, deadline, Stage::Open, async {
        ready.notified().await;
        Ok(())
    });
    tokio::select! {
        result = &mut connection => return Ok(result?),
        result = opening => result?,
    }
    Ok(connection.await?)
}

fn router<S>(
    service: S,
    stop: CancellationToken,
    ready: std::sync::Arc<tokio::sync::Notify>,
) -> impl hyper::service::Service<
    Request<hyper::body::Incoming>,
    Response = Response<Outgoing>,
    Error = crate::Error,
    Future: Send,
> + Clone
where
    S: Service<Request<Incoming>, Response = Response<Outgoing>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };
    let opened = Arc::new(AtomicU32::new(0));
    hyper::service::service_fn(move |request: Request<hyper::body::Incoming>| {
        ready.notify_one();
        let admitted = opened.try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            (count < crate::STREAMS).then_some(count + 1)
        });
        let service = service.clone();
        let stop = stop.clone();
        let request = request.map(|body| Incoming::h2(body, stop.clone()));
        async move {
            if admitted.is_err() {
                stop.cancel();
                return Err(ErrorKind::Protocol.into());
            }
            match service.oneshot(request).await {
                Ok(response) => Ok(response),
                Err(never) => match never {},
            }
        }
    })
}

pub(crate) async fn serve<M, S>(
    listener: TcpListener,
    identity: Identity,
    make: M,
    stop: CancellationToken,
) -> Result<()>
where
    M: Fn(ConnectionInfo) -> S + Send + Sync,
    S: Service<Request<Incoming>, Response = Response<Outgoing>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let tls = std::sync::Arc::new(identity.h2()?);
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            result = workers.join_next(), if !workers.is_empty() => {
                if let Some(Err(source)) = result { crate::server::observe(Err(source.into()), Transport::H2); }
            }
            accepted = listener.accept(), if workers.len() < 512 => {
                let (socket, peer) = accepted?;
                let child = stop.child_token();
                let service = make(ConnectionInfo { transport: Transport::H2, peer, stop: child.clone(), native_stats: None });
                let tls = std::sync::Arc::clone(&tls);
                workers.spawn(async move {
                    let _guard = child.clone().drop_guard();
                    crate::server::observe(Session { service, stop: child.clone() }.serve(socket, &tls).await, Transport::H2);
                });
            }
        }
    }
    crate::server::finish(&mut workers).await;
    Ok(())
}
