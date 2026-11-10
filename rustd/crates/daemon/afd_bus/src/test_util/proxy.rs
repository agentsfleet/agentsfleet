//! A TCP relay in front of one NATS node, so a test can cut a client's
//! connection and restore it without touching the shared cluster.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::{AbortHandle, JoinHandle},
};

/// A cut-able relay on a fresh local port. Dropping it stops accepting; a
/// relay already open runs until either side closes.
#[derive(Debug)]
pub struct Proxy {
    port: u16,
    cut: Arc<AtomicBool>,
    relays: Arc<Mutex<Vec<AbortHandle>>>,
    accept: JoinHandle<()>,
}

impl Proxy {
    /// Relay a fresh local port to `backend` on 127.0.0.1.
    ///
    /// # Errors
    /// The local listener could not bind.
    pub async fn start(backend: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let cut = Arc::new(AtomicBool::new(false));
        let relays = Arc::new(Mutex::new(Vec::new()));
        let accept = tokio::spawn(accept(
            listener,
            backend,
            Arc::clone(&cut),
            Arc::clone(&relays),
        ));
        Ok(Self {
            port,
            cut,
            relays,
            accept,
        })
    }

    /// The local port a client dials to reach the backend through the relay.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Drop every live connection and refuse new ones until [`Self::restore`].
    pub async fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        for relay in self.relays.lock().await.drain(..) {
            relay.abort();
        }
    }

    /// Accept connections again; the clients cut off reconnect on their own.
    pub fn restore(&self) {
        self.cut.store(false, Ordering::SeqCst);
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

async fn accept(
    listener: TcpListener,
    backend: u16,
    cut: Arc<AtomicBool>,
    relays: Arc<Mutex<Vec<AbortHandle>>>,
) {
    while let Ok((mut client, _)) = listener.accept().await {
        if cut.load(Ordering::SeqCst) {
            drop(client);
            continue;
        }
        let relay = tokio::spawn(async move {
            if let Ok(mut server) = TcpStream::connect(("127.0.0.1", backend)).await {
                let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
            }
        });
        relays.lock().await.push(relay.abort_handle());
    }
}
