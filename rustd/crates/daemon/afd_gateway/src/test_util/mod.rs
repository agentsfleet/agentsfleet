//! In-process gateway sessions for local measurement and fixtures.
//!
//! The measured gateway keeps its production cache, authority, dispatch and frame writers.
//! Listener authentication, the replica registry and scheduler have separate native tests.

use crate::session::{
    SchedulingState, SessionHandle, SessionServices,
    cache::LeaseCache,
    io::{self, IncomingFrame, StreamWriters},
    liveness::Liveness,
    opening::allowed,
    requests::RequestTracker,
};
use crate::{Gateway, Result, error::ErrorKind};
use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_stream::{Budget, OwnedFrame, Reader};
use af_wire::{
    lease::AcceptedKinds,
    protocol::{StreamKind, Up},
};
use afd_auth::directory::Digest;
use std::{collections::BTreeMap, num::NonZeroU64, sync::Arc};
use tokio::{
    io::DuplexStream,
    sync::{Notify, mpsc},
    task::JoinSet,
    time::Instant,
};
use tokio_util::sync::CancellationToken;

/// A local caller drives real gateway operations without network or background probes.
/// Dropping the session cancels and aborts its owned frame writers.
pub struct Session {
    services: Arc<SessionServices>,
    readers: BTreeMap<StreamKind, Reader<DuplexStream>>,
    budget: Budget,
    requests: Arc<RequestTracker>,
    workers: JoinSet<Result<()>>,
    revision: u64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("runner", &self.services.handle.runner)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Acquire the enrolled identity and rebuild its real lease targets before measurement.
    ///
    /// # Errors
    /// Preserves ownership, database and frame-assembly failures.
    pub async fn open(
        gateway: &Gateway,
        runner: Uuid7,
        digest: Digest,
        accepted: AcceptedKinds,
        credits: usize,
    ) -> Result<Self> {
        let dependencies = &gateway.inner.dependencies;
        let proposed = allowed(
            dependencies
                .runners
                .prepare_connection(&runner, digest, None, None, dependencies.wall_clock.now())
                .await?,
        )?;
        let authority = allowed(dependencies.runners.confirm_connection(proposed).await?)?;
        let cache = LeaseCache::rebuild(&dependencies.database, &runner).await?;
        let stop = CancellationToken::new();
        let budget = Budget::default();
        let pipes = Pipes::new(&stop, &budget)?;
        let handle = handle(&authority, credits, stop);
        let services = Arc::new(SessionServices::connected(
            gateway.clone(),
            handle,
            authority,
            cache,
            pipes.writers,
            accepted,
        ));
        let mut session = Self {
            services,
            readers: pipes.readers,
            budget,
            requests: Arc::new(RequestTracker::default()),
            workers: pipes.workers,
            revision: 0,
        };
        session.ready(credits).await?;
        Ok(session)
    }

    /// Advertise capacity through the production revision and observed-offer checks.
    ///
    /// # Errors
    /// Refuses exhausted revisions or invalid readiness frames.
    pub async fn ready(&mut self, credits: usize) -> Result<()> {
        self.revision = self.revision.checked_add(1).ok_or(ErrorKind::Protocol)?;
        let revision = NonZeroU64::new(self.revision).ok_or(ErrorKind::Protocol)?;
        let observed_offers = self.services.handle.snapshot.lock().offers_sent;
        self.send(&Up::Ready {
            credits,
            revision,
            observed_offers,
        })
        .await
    }

    /// Run one production offer pass entry, returning its charged encoded offer.
    ///
    /// # Errors
    /// Preserves issue and frame delivery failures.
    pub async fn offer(&mut self) -> Result<Option<OwnedFrame>> {
        let services = Arc::clone(&self.services);
        let reader = self
            .readers
            .get_mut(&StreamKind::Work)
            .ok_or(ErrorKind::Protocol)?;
        let offered = services.offer();
        let receive = reader.next();
        tokio::pin!(offered, receive);
        tokio::select! {
            result = &mut offered => {
                if result? { Ok(Some(receive.await?.ok_or(ErrorKind::Closed)?)) } else { Ok(None) }
            }
            frame = &mut receive => {
                let frame = frame?.ok_or(ErrorKind::Closed)?;
                if !offered.await? { return Err(ErrorKind::Protocol.into()); }
                Ok(Some(frame))
            }
        }
    }

    /// Dispatch one reply-bearing frame and read its fully written answer.
    ///
    /// # Errors
    /// Preserves dispatch errors and refuses missing or mismatched correlation.
    pub async fn request(&mut self, message: &Up<'_>) -> Result<OwnedFrame> {
        let id = message.request_id().ok_or(ErrorKind::Protocol)?;
        let kind = message.stream_kind();
        let incoming = self.incoming(message).await?;
        let services = Arc::clone(&self.services);
        let reader = self.readers.get_mut(&kind).ok_or(ErrorKind::Protocol)?;
        let ((), frame) = tokio::try_join!(
            Box::pin(services.dispatch(incoming, &self.requests)),
            async { Ok::<_, crate::Error>(reader.next().await?.ok_or(ErrorKind::Closed)?) },
        )?;
        let answer: af_wire::protocol::Down<'_> = frame.decode()?;
        if answer.request_id() != Some(id) {
            return Err(ErrorKind::Protocol.into());
        }
        Ok(frame)
    }

    /// Dispatch a one-way activity or readiness frame through the same decoder.
    ///
    /// # Errors
    /// Refuses a reply-bearing frame or any dispatch failure.
    pub async fn send(&mut self, message: &Up<'_>) -> Result<()> {
        if message.request_id().is_some() {
            return Err(ErrorKind::Protocol.into());
        }
        let incoming = self.incoming(message).await?;
        Box::pin(Arc::clone(&self.services).dispatch(incoming, &self.requests)).await
    }

    async fn incoming(&self, message: &Up<'_>) -> Result<IncomingFrame> {
        let kind = message.stream_kind();
        let frame = af_stream::encode(
            message,
            &self.budget,
            kind,
            &self.services.handle.stop,
            Instant::now() + STREAM_ANSWER_TIMEOUT,
        )
        .await?;
        Ok(IncomingFrame { kind, frame })
    }

    /// Join every writer before the benchmark releases its datastores.
    pub async fn close(mut self) {
        self.shutdown().await;
    }

    /// Cancel and join writers while retaining the caller's session allocation.
    pub async fn shutdown(&mut self) {
        self.services.handle.stop.cancel();
        self.workers.shutdown().await;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.services.handle.stop.cancel();
        self.workers.abort_all();
    }
}

/// No registry runs in this measurement; the session still owns its bounded writers.
struct Pipes {
    readers: BTreeMap<StreamKind, Reader<DuplexStream>>,
    writers: StreamWriters,
    workers: JoinSet<Result<()>>,
}

impl Pipes {
    fn new(stop: &CancellationToken, budget: &Budget) -> Result<Self> {
        let mut workers = JoinSet::new();
        let mut readers = BTreeMap::new();
        let mut writers = BTreeMap::new();
        for kind in StreamKind::ALL {
            let (output, input) = tokio::io::duplex(af_stream::CHUNK_BYTES);
            writers.insert(kind, io::writer(output, stop.clone(), &mut workers));
            readers.insert(
                kind,
                Reader::new(
                    input,
                    budget.clone(),
                    kind,
                    stop.clone(),
                    STREAM_ANSWER_TIMEOUT,
                ),
            );
        }
        Ok(Self {
            readers,
            writers: StreamWriters::try_from(writers)?,
            workers,
        })
    }
}

fn handle(
    authority: &afd_runner::capacity::authority::Authority,
    credits: usize,
    stop: CancellationToken,
) -> Arc<SessionHandle> {
    let (commands, _) = mpsc::channel(1);
    let (claims, _) = mpsc::channel(1);
    Arc::new(SessionHandle {
        runner: authority.runner().clone(),
        connection: authority.connection().clone(),
        stop,
        drain: CancellationToken::new(),
        liveness: Liveness::default(),
        superseded: CancellationToken::new(),
        offers: Notify::new(),
        claims,
        rechecks: Notify::new(),
        commands,
        snapshot: parking_lot::Mutex::new(SchedulingState {
            ready: false,
            degraded: false,
            credits,
            offers_sent: 0,
            offers_seen: 0,
            ready_revision: None,
            held: Vec::new(),
            last_offer: Instant::now(),
        }),
    })
}
