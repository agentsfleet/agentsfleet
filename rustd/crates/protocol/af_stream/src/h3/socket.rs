use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use tokio::{net::UdpSocket, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use crate::{Endpoint, Result, Stage, error::within, tls::Trust};

pub(super) struct Connected {
    pub(super) initial:
        tokio_quiche::InitialQuicConnection<UdpSocket, tokio_quiche::metrics::DefaultMetrics>,
    pub(super) receive: AbortOnDropHandle<Result<()>>,
    pub(super) shutdown: mpsc::Receiver<()>,
}

pub(super) async fn connect(
    endpoint: &Endpoint,
    trust: &Trust,
    stop: &CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<Connected> {
    let socket = open(endpoint, stop, deadline).await?;
    let local = socket.local_addr()?;
    let peer = socket.peer_addr()?;
    let mut config = super::trust::client(trust)?;
    let mut cid = [0; 16];
    af_crypto::entropy::Entropy::default().fill(&mut cid)?;
    let expected_ip = endpoint.host().parse::<IpAddr>().ok();
    let server_name = expected_ip.is_none().then_some(endpoint.host());
    let connection = tokio_quiche::quiche::connect_with_buffer_factory(
        server_name,
        &tokio_quiche::quiche::ConnectionId::from_ref(&cid),
        local,
        peer,
        &mut config,
    )?;
    let mut wrapped = tokio_quiche::quic::raw::wrap_quiche_conn(
        connection,
        tokio_quiche::socket::Socket {
            send: Arc::clone(&socket),
            recv: (),
            local_addr: local,
            peer_addr: peer,
            capabilities: tokio_quiche::socket::SocketCapabilities::default(),
        },
        tokio_quiche::metrics::DefaultMetrics,
    );
    // quiche's server-name option verifies DNS SANs. Literal origins must prove an IP SAN.
    if let Some(ip) = expected_ip {
        wrapped.conn.ssl_mut().verify_param_mut().set_ip(ip)?;
    }
    let receive = receive(socket, wrapped.incoming_tx, local, peer, stop.clone());
    Ok(Connected {
        initial: wrapped.conn,
        receive,
        shutdown: wrapped.worker_shutdown_rx,
    })
}

async fn open(
    endpoint: &Endpoint,
    stop: &CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<Arc<UdpSocket>> {
    within(stop, deadline, Stage::Connect, async {
        let peer = match endpoint.dial {
            Some(peer) => peer,
            None => tokio::net::lookup_host((endpoint.host(), endpoint.port()))
                .await?
                .next()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::AddrNotAvailable))?,
        };
        let bind = if peer.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = UdpSocket::bind(bind).await?;
        socket.connect(peer).await?;
        Ok(Arc::new(socket))
    })
    .await
}

pub(super) fn receive(
    socket: Arc<UdpSocket>,
    packets: mpsc::Sender<tokio_quiche::quic::Incoming>,
    local: SocketAddr,
    peer: SocketAddr,
    stop: CancellationToken,
) -> AbortOnDropHandle<Result<()>> {
    AbortOnDropHandle::new(tokio::spawn(async move {
        let _guard = stop.clone().drop_guard();
        let mut buffer = vec![0; 65_535];
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => return Ok(()),
                received = socket.recv(&mut buffer) => {
                    let length = received?;
                    let packet = tokio_quiche::quic::Incoming {
                        peer_addr: peer, local_addr: local, rx_time: None,
                        buf: buffer.get(..length).ok_or(crate::error::ErrorKind::Protocol)?.to_vec(), gro: None,
                        #[cfg(target_os = "linux")]
                        so_mark_data: None,
                    };
                    tokio::select! {
                        biased;
                        () = stop.cancelled() => return Ok(()),
                        result = packets.send(packet) => if result.is_err() { return Ok(()); },
                    }
                }
            }
        }
    }))
}
