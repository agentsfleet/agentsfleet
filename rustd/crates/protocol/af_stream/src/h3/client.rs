use http::{Request, Response};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
    time::Instant,
};
use tokio_quiche::{
    ClientH3Controller, ClientH3Driver,
    http3::driver::{ClientH3Event, H3Event, NewClientRequest},
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use crate::{
    Endpoint, Incoming, Outgoing, Result, Stage,
    client::Native,
    error::{ErrorKind, within},
    tls::Trust,
};

use super::telemetry::NetworkStats;

struct Open {
    id: u64,
    request: Request<Outgoing>,
    reply: oneshot::Sender<Result<Response<Incoming>>>,
}

pub(crate) struct Client {
    requests: mpsc::Sender<Open>,
    pub(crate) stats: watch::Receiver<NetworkStats>,
    pub(crate) key_exchange: watch::Receiver<Option<crate::tls::KeyExchange>>,
    opened: u32,
}

pub(crate) async fn connect(
    endpoint: &Endpoint,
    trust: &Trust,
    stop: CancellationToken,
    deadline: Instant,
) -> Result<(Native, AbortOnDropHandle<Result<()>>)> {
    let super::socket::Connected {
        mut initial,
        receive,
        shutdown,
    } = super::socket::connect(endpoint, trust, &stop, deadline).await?;
    let (driver, controller) = ClientH3Driver::new(super::settings());
    crate::tls::key_exchange::require_session(initial.ssl_mut())?;
    let (driver, _completion, stats, mut key_exchange) =
        super::driver::Driver::new(driver, stop.clone());
    let (_connection, handshake) = initial.handshake_fut(driver);
    let handshake = Box::pin(handshake);
    let running = within(&stop, deadline, Stage::Handshake, async {
        handshake
            .await
            .map_err(|source| ErrorKind::QuicHandshake { source }.into())
    })
    .await?;
    tokio_quiche::InitialQuicConnection::<UdpSocket, _>::resume(running);
    within(&stop, deadline, Stage::Handshake, async {
        while key_exchange.borrow().is_none() {
            key_exchange
                .changed()
                .await
                .map_err(|_| ErrorKind::Closed {
                    stage: Stage::Handshake,
                })?;
        }
        match *key_exchange.borrow() {
            Some(crate::tls::KeyExchange::X25519MlKem768) => Ok(()),
            Some(negotiated) => Err(ErrorKind::KeyExchangeRefused { negotiated }.into()),
            None => Err(ErrorKind::Closed {
                stage: Stage::Handshake,
            }
            .into()),
        }
    })
    .await?;
    let (requests, receiver) = mpsc::channel(4);
    let observed = super::telemetry::observe(
        stats.clone(),
        key_exchange.clone(),
        af_observability::metrics::label::fleet::StreamPeer::Client,
    );
    let task = supervise(controller, receiver, stop, receive, shutdown, observed);
    Ok((
        Native::H3(Client {
            requests,
            stats,
            key_exchange,
            opened: 0,
        }),
        task,
    ))
}

fn supervise(
    controller: ClientH3Controller,
    requests: mpsc::Receiver<Open>,
    stop: CancellationToken,
    receive: AbortOnDropHandle<Result<()>>,
    mut shutdown: mpsc::Receiver<()>,
    observed: AbortOnDropHandle<()>,
) -> AbortOnDropHandle<Result<()>> {
    AbortOnDropHandle::new(tokio::spawn(async move {
        let _guard = stop.clone().drop_guard();
        let outcome = run(controller, requests, stop.clone()).await;
        stop.cancel();
        let receiver = receive.await;
        let _closed = shutdown.recv().await;
        observed.await?;
        outcome?;
        receiver?
    }))
}

impl Client {
    pub(crate) async fn open(&mut self, request: Request<Outgoing>) -> Result<Response<Incoming>> {
        if self.opened == crate::STREAMS {
            return Err(ErrorKind::Protocol.into());
        }
        self.opened += 1;
        let (reply, receiver) = oneshot::channel();
        if self
            .requests
            .send(Open {
                id: u64::from(self.opened),
                request,
                reply,
            })
            .await
            .is_err()
        {
            return Err(ErrorKind::Closed { stage: Stage::Open }.into());
        }
        receiver.await?
    }
}

type Reply = oneshot::Sender<Result<Response<Incoming>>>;

enum Opening {
    Queued { id: u64, reply: Reply },
    Assigned { stream: u64, reply: Reply },
}

impl Opening {
    fn assign(self, id: u64, stream: u64) -> Result<Self> {
        match self {
            Self::Queued {
                id: expected,
                reply,
            } if id == expected => Ok(Self::Assigned { stream, reply }),
            _ => Err(ErrorKind::Protocol.into()),
        }
    }

    fn answer(
        self,
        headers: tokio_quiche::http3::driver::IncomingH3Headers,
        stop: &CancellationToken,
    ) -> Result<()> {
        let Self::Assigned { stream, reply } = self else {
            return Err(ErrorKind::Protocol.into());
        };
        if stream != headers.stream_id {
            return Err(ErrorKind::Protocol.into());
        }
        let head = super::headers::ResponseHead::try_from(headers.headers.as_slice())?;
        let response = head.attach(Incoming::h3(headers.recv, stop.clone()));
        let _caller_gone = reply.send(Ok(response));
        Ok(())
    }
}

struct Pending {
    opening: Option<Opening>,
    uploads: JoinSet<Result<()>>,
}

async fn run(
    mut controller: ClientH3Controller,
    mut requests: mpsc::Receiver<Open>,
    stop: CancellationToken,
) -> Result<()> {
    // `Client::open(&mut self)` permits one opening at a time; upload workers remain concurrent.
    let mut pending = Pending {
        opening: None,
        uploads: JoinSet::new(),
    };
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            result = crate::jobs::completed(&mut pending.uploads), if !pending.uploads.is_empty() => result?,
            event = controller.event_receiver_mut().recv() => {
                let event = event.ok_or(ErrorKind::Closed { stage: Stage::Read })?;
                pending.event(event, &stop)?;
            }
            request = requests.recv() => {
                let Some(request) = request else { return Ok(()); };
                pending.open(request, &controller)?;
            }
        }
    }
}

impl Pending {
    fn open(&mut self, open: Open, controller: &ClientH3Controller) -> Result<()> {
        if self.opening.is_some() {
            return Err(ErrorKind::Protocol.into());
        }
        let (parts, body) = open.request.into_parts();
        let headers = super::headers::Encoded::try_from(parts)?.into();
        let (writer, ready) = oneshot::channel();
        if controller
            .request_sender()
            .send(NewClientRequest {
                request_id: open.id,
                headers,
                body_writer: Some(writer),
            })
            .is_err()
        {
            return Err(ErrorKind::Closed { stage: Stage::Open }.into());
        }
        self.opening = Some(Opening::Queued {
            id: open.id,
            reply: open.reply,
        });
        self.uploads
            .spawn(async move { super::upload(body, ready.await?).await });
        Ok(())
    }

    fn event(&mut self, event: ClientH3Event, stop: &CancellationToken) -> Result<()> {
        match event {
            ClientH3Event::NewOutboundRequest {
                stream_id,
                request_id,
            } => {
                let opening = self.opening.take().ok_or(ErrorKind::Protocol)?;
                self.opening = Some(opening.assign(request_id, stream_id)?);
            }
            ClientH3Event::Core(H3Event::IncomingHeaders(headers)) => {
                self.opening
                    .take()
                    .ok_or(ErrorKind::Protocol)?
                    .answer(headers, stop)?;
            }
            ClientH3Event::Core(H3Event::ConnectionError(source)) => return Err(source.into()),
            ClientH3Event::Core(H3Event::ConnectionShutdown(Some(source))) => {
                return Err(source.into());
            }
            ClientH3Event::Core(
                H3Event::ConnectionShutdown(None)
                | H3Event::GoAway { .. }
                | H3Event::ResetStream { .. }
                | H3Event::StreamClosed { .. },
            ) => return Err(ErrorKind::Closed { stage: Stage::Read }.into()),
            ClientH3Event::Core(
                H3Event::IncomingSettings { .. } | H3Event::BodyBytesReceived { .. },
            ) => {}
            ClientH3Event::Core(H3Event::NewFlow { .. }) => return Err(ErrorKind::Protocol.into()),
        }
        Ok(())
    }
}
