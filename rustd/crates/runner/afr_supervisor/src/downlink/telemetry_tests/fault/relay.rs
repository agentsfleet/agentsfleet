//! UDP is unanswered; a watch switch cuts existing TCP carriers and rejects new ones.

use super::{Failure, Result};
use tokio::{
    net::{TcpListener, TcpStream, UdpSocket},
    sync::watch,
    task::JoinSet,
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

pub(super) struct Relay {
    pub(super) url: String,
    _udp: UdpSocket,
    available: watch::Sender<bool>,
    stop: CancellationToken,
    task: AbortOnDropHandle<std::io::Result<()>>,
}

impl Relay {
    pub(super) async fn start(backend: u16) -> Result<Self, Failure> {
        let listener = TcpListener::bind("[::1]:0").await?;
        let address = listener.local_addr()?;
        let udp = UdpSocket::bind(address).await?;
        let (available, changed) = watch::channel(true);
        let stop = CancellationToken::new();
        let task = AbortOnDropHandle::new(tokio::spawn(relay(
            listener,
            backend,
            changed,
            stop.clone(),
        )));
        Ok(Self {
            url: format!("https://localhost:{}", address.port()),
            _udp: udp,
            available,
            stop,
            task,
        })
    }

    pub(super) fn cut(&self) {
        self.available.send_replace(false);
    }
    pub(super) fn restore(&self) {
        self.available.send_replace(true);
    }

    pub(super) async fn close(mut self) -> Result<(), Failure> {
        self.stop.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(5), &mut self.task).await???;
        Ok(())
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn relay(
    listener: TcpListener,
    backend: u16,
    mut available: watch::Receiver<bool>,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => { workers.shutdown().await; return Ok(()); }
            joined = workers.join_next(), if !workers.is_empty() => {
                joined.ok_or_else(|| std::io::Error::other("missing relay worker"))??;
            }
            accepted = listener.accept() => {
                let (client, _) = accepted?;
                if *available.borrow_and_update() {
                    workers.spawn(pipe(client, backend, available.clone(), stop.clone()));
                } else {
                    workers.spawn(blackhole(client, available.clone(), stop.clone()));
                }
            }
        }
    }
}

async fn pipe(
    mut client: TcpStream,
    backend: u16,
    mut available: watch::Receiver<bool>,
    stop: CancellationToken,
) {
    let Ok(mut server) = TcpStream::connect((std::net::Ipv6Addr::LOCALHOST, backend)).await else {
        return;
    };
    tokio::select! {
        () = stop.cancelled() => {}
        _ = available.changed() => {}
        _ = tokio::io::copy_bidirectional(&mut client, &mut server) => {}
    }
}

async fn blackhole(
    _client: TcpStream,
    mut available: watch::Receiver<bool>,
    stop: CancellationToken,
) {
    tokio::select! { () = stop.cancelled() => {}, _ = available.changed() => {} }
}
