//! TCP reaches the real TLS server while UDP is deliberately left unanswered.

use af_stream::Endpoint;
use tokio::{
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinSet,
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use super::{Host, Result, deadline};

pub(crate) struct Blackhole {
    pub(crate) endpoint: Endpoint,
    _udp: UdpSocket,
    stop: CancellationToken,
    task: AbortOnDropHandle<std::io::Result<()>>,
}

impl Blackhole {
    pub(crate) async fn start(host: &Host) -> Result<Self> {
        let listener = TcpListener::bind("[::1]:0").await?;
        let address = listener.local_addr()?;
        let udp = UdpSocket::bind(address).await?;
        let backend = host
            .endpoint
            .uri()
            .port_u16()
            .ok_or("backend port missing")?;
        let endpoint = Endpoint::parse(&format!("https://localhost:{}", address.port()))?;
        let stop = host.stop.child_token();
        let task = AbortOnDropHandle::new(tokio::spawn(relay(listener, backend, stop.clone())));
        Ok(Self {
            endpoint,
            _udp: udp,
            stop,
            task,
        })
    }

    pub(crate) async fn close(mut self) -> Result {
        self.stop.cancel();
        tokio::time::timeout_at(deadline(), &mut self.task).await???;
        Ok(())
    }
}

impl Drop for Blackhole {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn relay(
    listener: TcpListener,
    backend: u16,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => { workers.shutdown().await; return Ok(()); },
            done = workers.join_next(), if !workers.is_empty() => {
                let joined = done.ok_or_else(|| std::io::Error::other("missing relay worker"))?;
                joined??;
            }
            accepted = listener.accept() => {
                let (mut client, _peer) = accepted?;
                workers.spawn(async move {
                    let mut server = TcpStream::connect((std::net::Ipv6Addr::LOCALHOST, backend)).await?;
                    tokio::io::copy_bidirectional(&mut client, &mut server).await?;
                    Ok::<_, std::io::Error>(())
                });
            }
        }
    }
}
