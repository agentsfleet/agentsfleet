//! A test-owned connection cut with an observable, fragment-safe trigger.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

pub(super) struct CutProxy {
    addr: SocketAddr,
    cuts: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}

impl CutProxy {
    pub(super) async fn new(target: SocketAddr, trigger: &'static [u8]) -> Self {
        assert_ne!(trigger.len(), 0);
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cuts = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&cuts);
        let worker = tokio::spawn(async move {
            let mut relays = JoinSet::new();
            while let Ok((client, _)) = listener.accept().await {
                relays.spawn(relay(client, target, trigger, Arc::clone(&observed)));
            }
        });
        Self {
            addr,
            cuts,
            worker: Some(worker),
        }
    }

    pub(super) const fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub(super) fn cuts(&self) -> usize {
        self.cuts.load(Ordering::Acquire)
    }

    pub(super) async fn close(mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
            assert!(worker.await.unwrap_err().is_cancelled());
        }
    }
}

impl Drop for CutProxy {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

async fn relay(client: TcpStream, target: SocketAddr, trigger: &[u8], cuts: Arc<AtomicUsize>) {
    let upstream = TcpStream::connect(target).await.unwrap();
    let (mut from_client, mut to_client) = client.into_split();
    let (mut from_server, mut to_server) = upstream.into_split();
    let requests = async {
        let mut buffer = [0; 8192];
        let mut pending = Vec::new();
        while let Ok(read) = from_client.read(&mut buffer).await {
            if read == 0 {
                return;
            }
            let bytes = buffer.get(..read).expect("read count fits its buffer");
            pending.extend_from_slice(bytes);
            if pending.windows(trigger.len()).any(|bytes| bytes == trigger) {
                cuts.fetch_add(1, Ordering::AcqRel);
                return;
            }
            if to_server.write_all(bytes).await.is_err() {
                return;
            }
            let keep = trigger.len() - 1;
            pending.drain(..pending.len().saturating_sub(keep));
        }
    };
    tokio::select! {
        () = requests => {},
        _copied = tokio::io::copy(&mut from_server, &mut to_client) => {},
    }
}
