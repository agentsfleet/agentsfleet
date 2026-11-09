//! One opaque relay permits UDP restoration and counts actual native connection paths.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinSet,
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

struct State {
    udp_enabled: AtomicBool,
    tcp: AtomicUsize,
    udp: AtomicUsize,
    tcp_closed: CancellationToken,
}

pub(super) struct Relay {
    pub(super) origin: String,
    state: Arc<State>,
    stop: CancellationToken,
    task: AbortOnDropHandle<std::io::Result<()>>,
}

impl Relay {
    pub(super) async fn start(backend: SocketAddr, udp_enabled: bool) -> Self {
        let listener = TcpListener::bind("[::1]:0").await.expect("relay TCP bind");
        let address = listener.local_addr().expect("relay address");
        let udp = Arc::new(UdpSocket::bind(address).await.expect("relay UDP bind"));
        let state = Arc::new(State {
            udp_enabled: AtomicBool::new(udp_enabled),
            tcp: AtomicUsize::new(0),
            udp: AtomicUsize::new(0),
            tcp_closed: CancellationToken::new(),
        });
        let stop = CancellationToken::new();
        let tcp = tcp_loop(listener, backend, Arc::clone(&state), stop.clone());
        let udp = udp_loop(udp, backend, Arc::clone(&state), stop.clone());
        let task = tokio::spawn(async move {
            tokio::try_join!(tcp, udp)?;
            Ok(())
        });
        Self {
            origin: format!("https://localhost:{}", address.port()),
            state,
            stop,
            task: AbortOnDropHandle::new(task),
        }
    }

    pub(super) fn enable_udp(&self) {
        self.state.udp_enabled.store(true, Ordering::SeqCst);
    }

    pub(super) fn end_tcp(&self) {
        self.state.tcp_closed.cancel();
    }

    pub(super) fn connections(&self) -> (usize, usize) {
        (
            self.state.tcp.load(Ordering::SeqCst),
            self.state.udp.load(Ordering::SeqCst),
        )
    }

    pub(super) async fn close(mut self) {
        self.stop.cancel();
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT, &mut self.task)
            .await
            .expect("relay shutdown deadline")
            .expect("relay task joins")
            .expect("relay I/O");
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn tcp_loop(
    listener: TcpListener,
    backend: SocketAddr,
    state: Arc<State>,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => { workers.shutdown().await; return Ok(()); }
            done = workers.join_next(), if !workers.is_empty() => {
                let _closed_connection = done.ok_or_else(|| std::io::Error::other("missing TCP relay worker"))?.map_err(std::io::Error::other)?;
            }
            accepted = listener.accept() => {
                let (mut client,_) = accepted?;
                state.tcp.fetch_add(1,Ordering::SeqCst);
                let state = Arc::clone(&state);
                let stop = stop.clone();
                workers.spawn(async move {
                    let mut server = TcpStream::connect(backend).await?;
                    tokio::select! {
                        biased;
                        () = stop.cancelled() => Ok(()),
                        () = state.tcp_closed.cancelled() => Ok(()),
                        copied = tokio::io::copy_bidirectional(&mut client,&mut server) => copied.map(|_bytes| ()),
                    }
                });
            }
        }
    }
}

async fn udp_loop(
    public: Arc<UdpSocket>,
    backend: SocketAddr,
    state: Arc<State>,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut paths = HashMap::<SocketAddr, Arc<UdpSocket>>::new();
    let mut workers = JoinSet::new();
    let mut bytes = vec![0; 65536];
    loop {
        let (count, client) = tokio::select! {
            biased;
            () = stop.cancelled() => { workers.shutdown().await; return Ok(()); }
            done = workers.join_next(), if !workers.is_empty() => {
                let _closed_path = done.ok_or_else(|| std::io::Error::other("missing UDP relay worker"))?.map_err(std::io::Error::other)?;
                continue;
            }
            packet = public.recv_from(&mut bytes) => packet?,
        };
        if !state.udp_enabled.load(Ordering::SeqCst) {
            continue;
        }
        if let std::collections::hash_map::Entry::Vacant(e) = paths.entry(client) {
            let upstream = Arc::new(UdpSocket::bind("[::1]:0").await?);
            upstream.connect(backend).await?;
            workers.spawn(udp_replies(
                Arc::clone(&upstream),
                Arc::clone(&public),
                client,
                stop.clone(),
            ));
            e.insert(upstream);
            state.udp.fetch_add(1, Ordering::SeqCst);
        }
        paths
            .get(&client)
            .ok_or_else(|| std::io::Error::other("missing UDP path"))?
            .send(bytes.get(..count).expect("bounded relay read"))
            .await?;
    }
}

async fn udp_replies(
    upstream: Arc<UdpSocket>,
    public: Arc<UdpSocket>,
    client: SocketAddr,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut bytes = vec![0; 65536];
    loop {
        let count = tokio::select! { biased; () = stop.cancelled() => return Ok(()), packet = upstream.recv(&mut bytes) => packet? };
        public
            .send_to(bytes.get(..count).expect("bounded relay read"), client)
            .await?;
    }
}
