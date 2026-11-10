//! An opaque TCP path forces the production runner's existing HTTP/2 fallback.

use std::{net::Ipv6Addr, time::Duration};

use tokio::{
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinSet,
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

type Failure = Box<dyn std::error::Error + Send + Sync>;

pub(crate) struct TcpOnly {
    pub(crate) url: String,
    _udp: UdpSocket,
    stop: CancellationToken,
    owner: AbortOnDropHandle<std::io::Result<()>>,
}

impl TcpOnly {
    pub(super) async fn start(origin: &str) -> Result<Self, Failure> {
        let backend = origin
            .parse::<http::Uri>()?
            .port_u16()
            .ok_or("native fixture port missing")?;
        let listener = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let udp = UdpSocket::bind(address).await?;
        let stop = CancellationToken::new();
        let owner = AbortOnDropHandle::new(tokio::spawn(serve(listener, backend, stop.clone())));
        Ok(Self {
            url: format!("https://localhost:{}", address.port()),
            _udp: udp,
            stop,
            owner,
        })
    }

    pub(crate) async fn close(mut self) -> Result<(), Failure> {
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), &mut self.owner).await???;
        Ok(())
    }
}

impl Drop for TcpOnly {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn serve(
    listener: TcpListener,
    backend: u16,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => { workers.shutdown().await; return Ok(()); }
            joined = workers.join_next(), if !workers.is_empty() => {
                let _closed = joined
                    .ok_or_else(|| std::io::Error::other("missing native TCP relay worker"))?
                    .map_err(std::io::Error::other)?;
            }
            accepted = listener.accept() => {
                let (client, _) = accepted?;
                workers.spawn(forward(client, backend, stop.clone()));
            }
        }
    }
}

async fn forward(
    mut client: TcpStream,
    backend: u16,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut server = TcpStream::connect((Ipv6Addr::LOCALHOST, backend)).await?;
    tokio::select! {
        () = stop.cancelled() => Ok(()),
        copied = tokio::io::copy_bidirectional(&mut client, &mut server) => copied.map(|_| ()),
    }
}
