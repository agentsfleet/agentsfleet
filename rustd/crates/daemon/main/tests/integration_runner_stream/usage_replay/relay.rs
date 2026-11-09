//! Forward real native requests and withhold one completed renewal answer before disconnecting.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::{
    Budget, Connection, ConnectionInfo, Endpoint, Incoming, Listener, Outgoing, Reader,
};
use af_wire::protocol::{self, Down, StreamKind};
use axum::http::{Request, Response};
use std::{
    convert::Infallible,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{sync::Mutex, task::JoinSet, time::Instant};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

#[derive(Default)]
struct Answers {
    armed: AtomicBool,
    lost: AtomicUsize,
    resumed: CancellationToken,
}

pub(super) struct Relay {
    pub(super) address: SocketAddr,
    identity: af_stream::test_util::IdentityFixture,
    answers: Arc<Answers>,
    stop: CancellationToken,
    owner: AbortOnDropHandle<af_stream::Result<()>>,
    jobs: Arc<Mutex<JoinSet<af_stream::Result<()>>>>,
}

impl Relay {
    pub(super) async fn start(backend: SocketAddr) -> Self {
        let identity = af_stream::test_util::IdentityFixture::new(vec!["::1".to_owned()])
            .expect("relay identity");
        let listener = Listener::bind(
            "[::1]:0".parse().expect("relay address"),
            identity.identity(),
        )
        .await
        .expect("native relay binds");
        let address = listener.local_addr().expect("relay address");
        let endpoint = Endpoint::parse(&format!("https://[::1]:{}", backend.port()))
            .expect("daemon native origin");
        let answers = Arc::new(Answers::default());
        let jobs = Arc::new(Mutex::new(JoinSet::new()));
        let (faults, children) = (Arc::clone(&answers), Arc::clone(&jobs));
        let make = move |info: ConnectionInfo| {
            let peer = Arc::new(Peer {
                info,
                endpoint: endpoint.clone(),
                connection: Mutex::new(None),
                answers: Arc::clone(&faults),
                jobs: Arc::clone(&children),
            });
            tower::service_fn(move |request| {
                let peer = Arc::clone(&peer);
                async move { Ok::<_, Infallible>(peer.forward(request).await) }
            })
        };
        let stop = CancellationToken::new();
        let owner = AbortOnDropHandle::new(tokio::spawn(listener.serve(
            make,
            stop.clone(),
            stop.clone(),
        )));
        Self {
            address,
            identity,
            answers,
            stop,
            owner,
            jobs,
        }
    }

    pub(super) fn root(&self) -> PathBuf {
        self.identity.root()
    }

    pub(super) fn lose_next_renewal(&self) {
        self.answers.armed.store(true, Ordering::SeqCst);
    }

    pub(super) fn resume(&self) {
        self.answers.resumed.cancel();
    }

    pub(super) async fn lost(&self) {
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT, async {
            while self.answers.lost.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the relay receives the committed renewal answer");
        assert_eq!(self.answers.lost.load(Ordering::SeqCst), 1);
    }

    pub(super) async fn close(mut self) {
        self.stop.cancel();
        self.answers.resumed.cancel();
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT, &mut self.owner)
            .await
            .expect("relay stops")
            .expect("relay joins")
            .expect("native listener");
        self.jobs.lock().await.shutdown().await;
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.cancel();
        self.answers.resumed.cancel();
    }
}

struct Peer {
    info: ConnectionInfo,
    endpoint: Endpoint,
    connection: Mutex<Option<Connection>>,
    answers: Arc<Answers>,
    jobs: Arc<Mutex<JoinSet<af_stream::Result<()>>>>,
}

impl Peer {
    async fn forward(self: Arc<Self>, request: Request<Incoming>) -> Response<Outgoing> {
        let kind = StreamKind::parse(
            request.headers()[protocol::STREAM_KIND_HEADER]
                .to_str()
                .expect("stream kind text"),
        )
        .expect("stream kind");
        let (mut parts, body) = request.into_parts();
        parts.uri = self.endpoint.uri().clone();
        let request = Request::from_parts(parts, Outgoing::new(body));
        let response = self.open(request).await;
        let (parts, incoming) = response.into_parts();
        let (writer, body) = Outgoing::pipe();
        let peer = Arc::clone(&self);
        self.jobs
            .lock()
            .await
            .spawn(async move { peer.replies(incoming, writer, kind).await });
        Response::from_parts(parts, body)
    }

    async fn open(&self, request: Request<Outgoing>) -> Response<Incoming> {
        if self.answers.lost.load(Ordering::SeqCst) != 0 {
            self.answers.resumed.cancelled().await;
        }
        let mut slot = self.connection.lock().await;
        if slot.is_none() {
            *slot = Some(
                Connection::connect(
                    self.info.transport,
                    &self.endpoint,
                    &crate::support::runner_stream::trust(),
                    &self.info.stop,
                    Instant::now() + STREAM_ANSWER_TIMEOUT,
                )
                .await
                .expect("relay connects to the actual daemon"),
            );
        }
        slot.as_mut()
            .expect("native connection is present")
            .open(request, Instant::now() + STREAM_ANSWER_TIMEOUT)
            .await
            .expect("real gateway opens its stream")
    }

    async fn replies(
        &self,
        incoming: Incoming,
        mut writer: tokio::io::DuplexStream,
        kind: StreamKind,
    ) -> af_stream::Result<()> {
        let mut reader = Reader::new(
            incoming.reader(),
            Budget::default(),
            kind,
            self.info.stop.clone(),
            STREAM_ANSWER_TIMEOUT,
        );
        loop {
            let next = reader.next().await?;
            let Some(frame) = next else { break };
            let message: Down<'_> = frame.decode()?;
            if matches!(message, Down::Renewed { .. })
                && self.answers.armed.swap(false, Ordering::SeqCst)
            {
                // Receiving this answer proves both real lease transactions completed; transport acknowledgements stayed available for the large request.
                self.answers.lost.fetch_add(1, Ordering::SeqCst);
                self.info.stop.cancel();
                return Ok(());
            }
            af_stream::write(
                &mut writer,
                &frame,
                &self.info.stop,
                Instant::now() + STREAM_ANSWER_TIMEOUT,
            )
            .await?;
        }
        Ok(())
    }
}
