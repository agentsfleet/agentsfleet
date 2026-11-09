//! Opaque TCP and UDP relays inject packet loss without replacing either native endpoint.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::Transport;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinSet,
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

#[derive(Default)]
struct State {
    cut: AtomicBool,
    generation: AtomicUsize,
    tcp_connections: AtomicUsize,
    udp_paths: AtomicUsize,
    dropped: AtomicUsize,
}

impl State {
    fn forwards(&self, generation: usize) -> bool {
        !self.cut.load(Ordering::SeqCst) && self.generation.load(Ordering::SeqCst) == generation
    }

    fn dropped(&self, bytes: usize) {
        self.dropped.fetch_add(bytes, Ordering::SeqCst);
    }
}

pub(in crate::integration_runner_stream) struct FaultProxy {
    pub(in crate::integration_runner_stream) origin: String,
    state: Arc<State>,
    stop: CancellationToken,
    task: AbortOnDropHandle<std::io::Result<()>>,
}

impl FaultProxy {
    pub(in crate::integration_runner_stream) async fn start(
        backend: SocketAddr,
        carrier: Transport,
    ) -> Self {
        let listener = TcpListener::bind("[::1]:0").await.expect("relay TCP bind");
        let address = listener.local_addr().expect("relay address");
        let udp = Arc::new(UdpSocket::bind(address).await.expect("relay UDP bind"));
        let state = Arc::new(State::default());
        let stop = CancellationToken::new();
        let tcp = tcp_loop(listener, backend, Arc::clone(&state), stop.clone());
        let udp = udp_loop(
            udp,
            backend,
            carrier == Transport::H3,
            Arc::clone(&state),
            stop.clone(),
        );
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

    pub(super) fn blackhole(&self) {
        self.state.cut.store(true, Ordering::SeqCst);
    }

    /// Old paths remain lost; only a newly established carrier can reach the daemon again.
    pub(super) fn restore_new_connections(&self) {
        self.state.generation.fetch_add(1, Ordering::SeqCst);
        self.state.cut.store(false, Ordering::SeqCst);
    }

    pub(super) fn dropped_bytes(&self) -> usize {
        self.state.dropped.load(Ordering::SeqCst)
    }

    pub(in crate::integration_runner_stream) fn assert_initial_carrier(&self, carrier: Transport) {
        match carrier {
            Transport::H2 => assert!(self.state.tcp_connections.load(Ordering::SeqCst) > 0),
            Transport::H3 => {
                assert!(self.state.udp_paths.load(Ordering::SeqCst) > 0);
                assert_eq!(self.state.tcp_connections.load(Ordering::SeqCst), 0);
            }
        }
    }

    pub(in crate::integration_runner_stream) async fn close(mut self) {
        self.stop.cancel();
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT, &mut self.task)
            .await
            .expect("relay closes")
            .expect("relay task joins")
            .expect("relay I/O");
    }
}

impl Drop for FaultProxy {
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
    let mut jobs = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => { jobs.shutdown().await; return Ok(()); }
            done = jobs.join_next(), if !jobs.is_empty() => {
                // Connection resets are expected after opaque ciphertext is dropped; task panics are not.
                let _connection_ended = done.ok_or_else(|| std::io::Error::other("missing TCP relay task"))?.map_err(std::io::Error::other)?;
            }
            accepted = listener.accept() => {
                let (client, _) = accepted?;
                state.tcp_connections.fetch_add(1, Ordering::SeqCst);
                let state = Arc::clone(&state);
                let stop = stop.clone();
                let generation = state.generation.load(Ordering::SeqCst);
                jobs.spawn(async move {
                    let server = TcpStream::connect(backend).await?;
                    let (client_read, client_write) = client.into_split();
                    let (server_read, server_write) = server.into_split();
                    tokio::try_join!(
                        pump(client_read, server_write, Arc::clone(&state), generation, stop.clone()),
                        pump(server_read, client_write, state, generation, stop)
                    )?;
                    Ok::<_, std::io::Error>(())
                });
            }
        }
    }
}

async fn pump<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut input: R,
    mut output: W,
    state: Arc<State>,
    generation: usize,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut bytes = [0; 8192];
    loop {
        let count = tokio::select! { biased; () = stop.cancelled() => return Ok(()), count = input.read(&mut bytes) => count? };
        if count == 0 {
            output.shutdown().await?;
            return Ok(());
        }
        if state.forwards(generation) {
            tokio::select! { biased; () = stop.cancelled() => return Ok(()), sent = output.write_all(bytes.get(..count).expect("bounded relay read")) => sent? }
        } else {
            state.dropped(count);
        }
    }
}

async fn udp_loop(
    socket: Arc<UdpSocket>,
    backend: SocketAddr,
    enabled: bool,
    state: Arc<State>,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut paths = HashMap::<SocketAddr, (Arc<UdpSocket>, usize)>::new();
    let mut jobs = JoinSet::new();
    let mut bytes = vec![0; 65536];
    loop {
        let (count, client) = tokio::select! {
            biased;
            () = stop.cancelled() => { jobs.shutdown().await; return Ok(()); }
            done = jobs.join_next(), if !jobs.is_empty() => {
                let _path_ended = done.ok_or_else(|| std::io::Error::other("missing UDP relay task"))?.map_err(std::io::Error::other)?;
                continue;
            }
            packet = socket.recv_from(&mut bytes) => packet?,
        };
        if !enabled || state.cut.load(Ordering::SeqCst) {
            state.dropped(count);
            continue;
        }
        if let std::collections::hash_map::Entry::Vacant(e) = paths.entry(client) {
            let upstream = udp_upstream(backend).await?;
            let generation = state.generation.load(Ordering::SeqCst);
            jobs.spawn(udp_replies(
                Arc::clone(&upstream),
                Arc::clone(&socket),
                client,
                Arc::clone(&state),
                generation,
                stop.clone(),
            ));
            e.insert((upstream, generation));
            state.udp_paths.fetch_add(1, Ordering::SeqCst);
        }
        let (upstream, generation) = paths
            .get(&client)
            .ok_or_else(|| std::io::Error::other("missing UDP relay path"))?;
        if state.forwards(*generation) {
            upstream
                .send(bytes.get(..count).expect("bounded relay read"))
                .await?;
        } else {
            state.dropped(count);
        }
    }
}

async fn udp_replies(
    upstream: Arc<UdpSocket>,
    public: Arc<UdpSocket>,
    client: SocketAddr,
    state: Arc<State>,
    generation: usize,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut bytes = vec![0; 65536];
    loop {
        let count = tokio::select! { biased; () = stop.cancelled() => return Ok(()), packet = upstream.recv(&mut bytes) => packet? };
        if state.forwards(generation) {
            public
                .send_to(bytes.get(..count).expect("bounded relay read"), client)
                .await?;
        } else {
            state.dropped(count);
        }
    }
}

async fn udp_upstream(backend: SocketAddr) -> std::io::Result<Arc<UdpSocket>> {
    let upstream = Arc::new(UdpSocket::bind("[::1]:0").await?);
    upstream.connect(backend).await?;
    Ok(upstream)
}
