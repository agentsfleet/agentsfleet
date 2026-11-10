use http::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpStream, time::Instant};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use crate::{
    Endpoint, Incoming, Outgoing, Result, Stage,
    client::Native,
    error::{ErrorKind, within},
    tls::Trust,
};

use super::executor::Executor;

pub(crate) struct Client {
    sender: hyper::client::conn::http2::SendRequest<Outgoing>,
    opened: u32,
    pub(crate) key_exchange: crate::tls::KeyExchange,
}

pub(crate) async fn connect(
    endpoint: &Endpoint,
    trust: &Trust,
    stop: CancellationToken,
    deadline: Instant,
) -> Result<(Native, AbortOnDropHandle<Result<()>>)> {
    let tls = secure(endpoint, trust, &stop, deadline).await?;
    let key_exchange = crate::tls::KeyExchange::from(tls.ssl()).require(
        crate::Transport::H2,
        af_observability::metrics::label::fleet::StreamPeer::Client,
    )?;
    let executor = Executor::new(stop.clone());
    let (sender, connection) = within(&stop, deadline, Stage::Handshake, async {
        Ok(hyper::client::conn::http2::Builder::new(executor.clone())
            .initial_stream_window_size(crate::STREAM_WINDOW)
            .initial_connection_window_size(crate::CONNECTION_WINDOW)
            .max_concurrent_streams(crate::STREAMS)
            .max_header_list_size(crate::HEADER_BYTES)
            .max_send_buf_size(crate::CHUNK_BYTES)
            .handshake(TokioIo::new(tls))
            .await?)
    })
    .await?;
    let task = tokio::spawn(async move { executor.run(async { Ok(connection.await?) }).await });
    Ok((
        Native::H2(Client {
            sender,
            opened: 0,
            key_exchange,
        }),
        AbortOnDropHandle::new(task),
    ))
}

async fn secure(
    endpoint: &Endpoint,
    trust: &Trust,
    stop: &CancellationToken,
    deadline: Instant,
) -> Result<tokio_boring::SslStream<TcpStream>> {
    let connector = trust.h2()?.configure()?;
    let socket = within(stop, deadline, Stage::Connect, async {
        crate::proxy::socket(endpoint).await
    })
    .await?;
    socket.set_nodelay(true)?;
    let tls = within(stop, deadline, Stage::Handshake, async {
        Ok(tokio_boring::connect(connector, endpoint.host(), socket).await?)
    })
    .await?;
    if tls.ssl().selected_alpn_protocol() != Some(b"h2".as_slice()) {
        return Err(ErrorKind::Protocol.into());
    }
    Ok(tls)
}

impl Client {
    pub(crate) async fn open(
        &mut self,
        request: Request<Outgoing>,
        stop: CancellationToken,
    ) -> Result<Response<Incoming>> {
        if self.opened == crate::STREAMS {
            return Err(ErrorKind::Protocol.into());
        }
        self.opened += 1;
        self.sender.ready().await?;
        Ok(self
            .sender
            .send_request(request)
            .await?
            .map(|body| Incoming::h2(body, stop)))
    }
}
