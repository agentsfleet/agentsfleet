use std::{convert::Infallible, ops::ControlFlow};

use futures_util::{SinkExt, StreamExt};
use http::{Request, Response};
use tokio::{net::UdpSocket, task::JoinSet};
use tokio_quiche::{
    ServerH3Controller, ServerH3Driver,
    http3::driver::{H3Event, IncomingH3Headers, OutboundFrame, ServerH3Event},
    metrics::DefaultMetrics,
    settings::{CertificateKind, TlsCertificatePaths},
};
use tokio_util::sync::{CancellationToken, DropGuard};
use tower::{Service, ServiceExt};

use crate::{
    Incoming, Outgoing, Result, Stage, Transport, error::ErrorKind, server::ConnectionInfo,
    tls::Identity,
};

type Accepted = tokio_quiche::InitialQuicConnection<UdpSocket, DefaultMetrics>;

pub(crate) async fn serve<M, S>(
    socket: UdpSocket,
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
    let mut incoming = listen(socket, &identity)?;
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            result = workers.join_next(), if !workers.is_empty() => {
                if let Some(Err(source)) = result { crate::server::observe(Err(source.into()), Transport::H3); }
            }
            accepted = incoming.next(), if workers.len() < 512 => match accepted {
                Some(Ok(accepted)) => { workers.spawn(Session::new(accepted, &make, stop.child_token()).serve()); }
                Some(Err(source)) => crate::server::observe(Err(ErrorKind::QuicListener { source }.into()), Transport::H3),
                None => return Err(ErrorKind::Closed { stage: Stage::Connect }.into()),
            }
        }
    }
    drop(incoming);
    crate::server::finish(&mut workers).await;
    Ok(())
}

fn listen(
    socket: UdpSocket,
    identity: &Identity,
) -> Result<tokio_quiche::QuicConnectionStream<DefaultMetrics>> {
    let paths = TlsCertificatePaths {
        cert: identity.chain.to_str().ok_or(ErrorKind::IdentityPath)?,
        private_key: identity.key.to_str().ok_or(ErrorKind::IdentityPath)?,
        kind: CertificateKind::X509,
    };
    let params = tokio_quiche::ConnectionParams::new_server(
        super::trust::server(),
        paths,
        tokio_quiche::settings::Hooks::default(),
    );
    let mut listeners = tokio_quiche::listen([socket], params, DefaultMetrics)
        .map_err(|source| ErrorKind::QuicListener { source })?;
    listeners.pop().ok_or_else(|| ErrorKind::Protocol.into())
}

struct Session<S> {
    controller: ServerH3Controller,
    service: S,
    stop: CancellationToken,
    completion: tokio::sync::oneshot::Receiver<()>,
    observed: tokio_util::task::AbortOnDropHandle<()>,
    handlers: JoinSet<Result<()>>,
    opened: u32,
    _guard: DropGuard,
}

impl<S> Session<S>
where
    S: Service<Request<Incoming>, Response = Response<Outgoing>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    fn new(
        accepted: Accepted,
        make: &impl Fn(ConnectionInfo) -> S,
        stop: CancellationToken,
    ) -> Self {
        let guard = stop.clone().drop_guard();
        let (driver, controller) = ServerH3Driver::new(super::settings());
        let (driver, completion, stats) = super::driver::Driver::new(driver, stop.clone());
        let observed = super::telemetry::observe(stats.clone());
        let service = make(ConnectionInfo {
            transport: Transport::H3,
            peer: accepted.peer_addr(),
            stop: stop.clone(),
            native_stats: Some(stats),
        });
        let _native = accepted.start(driver);
        Self {
            controller,
            service,
            stop,
            completion,
            observed,
            handlers: JoinSet::new(),
            opened: 0,
            _guard: guard,
        }
    }

    async fn serve(mut self) {
        let result = self.run().await;
        self.stop.cancel();
        self.handlers.shutdown().await;
        let _worker_dropped = self.completion.await;
        if let Err(error) = self.observed.await {
            crate::server::observe(Err(error.into()), Transport::H3);
        }
        crate::server::observe(result, Transport::H3);
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                biased;
                () = self.stop.cancelled() => return Ok(()),
                result = crate::jobs::completed(&mut self.handlers), if !self.handlers.is_empty() => result?,
                event = self.controller.event_receiver_mut().recv() => {
                    let event = event.ok_or(ErrorKind::Closed { stage: Stage::Read })?;
                    if self.dispatch(event)?.is_break() { return Ok(()); }
                }
            }
        }
    }

    fn dispatch(&mut self, event: ServerH3Event) -> Result<ControlFlow<()>> {
        match event {
            ServerH3Event::Headers {
                incoming_headers,
                is_in_early_data,
                ..
            } => {
                if self.opened == crate::STREAMS || *is_in_early_data {
                    return Err(ErrorKind::Protocol.into());
                }
                self.opened += 1;
                self.handlers.spawn(answer(
                    incoming_headers,
                    self.service.clone(),
                    self.stop.clone(),
                ));
                Ok(ControlFlow::Continue(()))
            }
            ServerH3Event::Core(H3Event::ConnectionError(source)) => Err(source.into()),
            ServerH3Event::Core(H3Event::ConnectionShutdown(Some(source))) => Err(source.into()),
            ServerH3Event::Core(
                H3Event::ConnectionShutdown(None)
                | H3Event::GoAway { .. }
                | H3Event::ResetStream { .. }
                | H3Event::StreamClosed { .. },
            ) => Ok(ControlFlow::Break(())),
            ServerH3Event::Core(
                H3Event::IncomingSettings { .. } | H3Event::BodyBytesReceived { .. },
            ) => Ok(ControlFlow::Continue(())),
            ServerH3Event::Core(H3Event::IncomingHeaders(_) | H3Event::NewFlow { .. }) => {
                Err(ErrorKind::Protocol.into())
            }
        }
    }
}

async fn answer<S>(headers: IncomingH3Headers, service: S, stop: CancellationToken) -> Result<()>
where
    S: Service<Request<Incoming>, Response = Response<Outgoing>, Error = Infallible>,
{
    let request = super::headers::RequestHead::try_from(headers.headers.as_slice())?
        .attach(Incoming::h3(headers.recv, stop));
    let response = match service.oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let (parts, body) = response.into_parts();
    let mut output = headers.send;
    let response = super::headers::Encoded::from(parts).into();
    output.send(OutboundFrame::Headers(response, None)).await?;
    super::upload(body, output).await
}
