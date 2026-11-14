//! Both listeners feed the same typed service factory, once per connection.

use std::{convert::Infallible, fmt, io, net::SocketAddr};

use http::{Request, Response};
use tokio::{
    net::{TcpListener, UdpSocket},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use tower::Service;

use crate::{Incoming, Outgoing, Result, Transport, tls::Identity};

/// Extra draws port zero gets when the port TCP picked is already held by another UDP socket.
/// The kernel hands out TCP and UDP ephemeral ports independently, so concurrent listeners on
/// one host collide now and then; each retry draws a fresh pair.
const PORT_ZERO_RETRIES: usize = 15;

/// Server-owned identity for one native connection, shared by its four streams.
#[derive(Clone, Debug)]
pub struct ConnectionInfo {
    /// Native transport selected during handshake.
    pub transport: Transport,
    /// Remote address; never use it as a metric label.
    pub peer: SocketAddr,
    /// Cancels every stream attached to this native connection.
    pub stop: CancellationToken,
    /// Live QUIC counters; HTTP/2 connections have no QUIC counters.
    pub native_stats: Option<tokio::sync::watch::Receiver<crate::NetworkStats>>,
}

/// TCP and UDP bound to one address, with a verified server identity.
pub struct Listener {
    tcp: TcpListener,
    udp: UdpSocket,
    identity: Identity,
    /// The UDP payload size HTTP/3 sends.
    datagram: usize,
}

impl fmt::Debug for Listener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Listener").finish_non_exhaustive()
    }
}

impl Listener {
    /// Binding port zero chooses TCP's port and binds UDP to that same port, drawing a fresh
    /// port when another socket already holds that UDP port. A fixed port is bound once.
    ///
    /// # Errors
    /// Returns an invalid server identity or failure to bind either socket.
    pub async fn bind(address: SocketAddr, identity: Identity) -> Result<Self> {
        let _checked = identity.h2()?;
        let (tcp, udp) = bind_pair(address, UdpSocket::bind::<SocketAddr>).await?;
        Ok(Self {
            tcp,
            udp,
            identity,
            datagram: crate::mtu::MAX_DATAGRAM_BYTES,
        })
    }

    /// Send HTTP/3 datagrams of at most `bytes`, to model a daemon with no ceiling of its own.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub const fn with_datagram_limit(mut self, bytes: usize) -> Self {
        self.datagram = bytes;
        self
    }

    /// Bound address shared by both transports.
    ///
    /// # Errors
    /// Returns a failure to inspect the bound TCP listener.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.tcp.local_addr()?)
    }

    /// Build one service per connection; all lanes reuse that service's context.
    /// Cancel `accepting` to drain connections, then `abort` if the caller's drain deadline expires.
    ///
    /// # Errors
    /// Returns listener setup or acceptance failures; individual peer failures are observed separately.
    pub async fn serve<M, S>(
        self,
        make: M,
        accepting: CancellationToken,
        abort: CancellationToken,
    ) -> Result<()>
    where
        M: Fn(ConnectionInfo) -> S + Clone + Send + Sync + 'static,
        S: Service<Request<Incoming>, Response = Response<Outgoing>, Error = Infallible>
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        let _guard = abort.clone().drop_guard();
        let h2 = crate::h2::serve(
            self.tcp,
            self.identity.clone(),
            make.clone(),
            accepting.clone(),
            abort.clone(),
        );
        let h3 = crate::h3::serve(
            self.udp,
            self.identity,
            self.datagram,
            make,
            accepting,
            abort.clone(),
        );
        // A failed carrier stops its sibling, but both owners still finish their cleanup.
        let h2 = async {
            let result = h2.await;
            if result.is_err() {
                abort.cancel();
            }
            result
        };
        let h3 = async {
            let result = h3.await;
            if result.is_err() {
                abort.cancel();
            }
            result
        };
        let (h2, h3) = tokio::join!(h2, h3);
        match (h2, h3) {
            (Ok(()), result) | (result, Ok(())) => result,
            (Err(primary), Err(secondary)) => {
                observe(Err(secondary), Transport::H3);
                Err(primary)
            }
        }
    }
}

/// TCP on `address`, then UDP on the port TCP got. Only port zero retries, and only when UDP
/// reports the port taken, so a fixed port another process holds still fails at once.
/// `bind_udp` is the system boundary, passed in so the collision can be staged.
async fn bind_pair<U, F>(address: SocketAddr, mut bind_udp: U) -> Result<(TcpListener, UdpSocket)>
where
    U: FnMut(SocketAddr) -> F,
    F: Future<Output = io::Result<UdpSocket>>,
{
    let retries = if address.port() == 0 {
        PORT_ZERO_RETRIES
    } else {
        0
    };
    for _ in 0..retries {
        let tcp = TcpListener::bind(address).await?;
        let udp = match bind_udp(tcp.local_addr()?).await {
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            bound => bound?,
        };
        return Ok((tcp, udp));
    }
    let tcp = TcpListener::bind(address).await?;
    let udp = bind_udp(tcp.local_addr()?).await?;
    Ok((tcp, udp))
}

pub(crate) fn observe(result: Result<()>, transport: Transport) {
    if let Err(error) = result {
        let transport = transport.as_str();
        let reason = error.reason().as_str();
        let io_kind = error.io_kind().map(|kind| format!("{kind:?}"));
        let error_code = error.code().as_str();
        tracing::warn!(
            event = "runner_stream_transport_ended",
            transport,
            reason,
            io_kind,
            error_code
        );
    }
}

/// The caller owns the absolute shutdown bound and cancels `abort` when it expires.
pub(crate) async fn finish(
    workers: &mut JoinSet<()>,
    abort: &CancellationToken,
    transport: Transport,
) {
    let joined = async {
        while let Some(done) = workers.join_next().await {
            if let Err(source) = done {
                observe(Err(source.into()), transport);
            }
        }
    };
    tokio::select! {
        () = joined => {},
        () = abort.cancelled() => workers.shutdown().await,
    }
}

#[cfg(test)]
mod tests;
