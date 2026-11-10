//! Provisional and committed session states each have one small, explicit ownership transition.

use super::{
    ConfirmedSession, Message, SchedulingState, SessionHandle, SessionOpening, SessionServices,
    commands,
    events::EventSource,
    opening,
    streams::{SessionStreams, StreamSetup},
    supervisor::SessionSupervisor,
};
use crate::Result;
use af_wire::protocol::{PENDING_MAX, StreamKind};
use std::sync::Arc;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

impl SessionOpening {
    /// Any failure or dropped opening cancels all native streams, including a partly read hello.
    pub(super) async fn run(self) -> Result<()> {
        let _cancel_on_exit = self.context.native.stop.clone().drop_guard();
        self.confirm().await?.run().await
    }
}

impl ConfirmedSession {
    /// Registration and the admission permit live until every supervised task has joined.
    async fn run(mut self) -> Result<()> {
        let (commands_tx, commands_rx) = mpsc::channel(PENDING_MAX);
        let (claims_tx, claims_rx) = mpsc::channel(1);
        let handle = Arc::new(self.handle(commands_tx, claims_tx));
        let _registration = self
            .gateway
            .inner
            .registry
            .register_current(
                &handle,
                &self.authority,
                &self.gateway.inner.dependencies.database,
            )
            .await?;
        if let Err(error) = commands::announce(&self.gateway, &handle.runner).await {
            // Ownership already committed; heartbeat and grant checks still fence the previous process.
            crate::Gateway::observe(&error, crate::Operation::Recheck);
        }
        self.welcome().await?;
        let setup = StreamSetup {
            budget: self.gateway.inner.input_budget.clone(),
            stop: handle.stop.clone(),
            deadline: self.deadline,
        };
        let mut streams = SessionStreams::attach(self.io, self.attachments, setup).await?;
        let services = Arc::new(SessionServices::connected(
            self.gateway.clone(),
            Arc::clone(&handle),
            self.authority,
            self.cache,
            streams.writers,
            self.hello.accepted,
        ));
        reconcile(&services, &self.hello.running).await?;
        streams
            .workers
            .spawn(commands::listen(Arc::clone(&services)));
        let mut supervisor = SessionSupervisor::new(services);
        let mut events = EventSource {
            frames: streams.frames,
            commands: commands_rx,
            claims: claims_rx,
        };
        let result = supervisor
            .run(&mut events, &mut streams.workers, &streams.read_stop)
            .await;
        handle.stop.cancel();
        supervisor.jobs.shutdown().await;
        streams.workers.shutdown().await;
        drop(self.permit);
        result
    }

    fn handle(
        &self,
        commands: mpsc::Sender<afd_bus::Command>,
        claims: mpsc::Sender<tokio::sync::oneshot::Sender<bool>>,
    ) -> SessionHandle {
        SessionHandle {
            runner: self.runner.id().clone(),
            connection: self.authority.connection().clone(),
            stop: self.context.native.stop.clone(),
            drain: CancellationToken::new(),
            liveness: super::liveness::Liveness::default(),
            superseded: CancellationToken::new(),
            offers: Notify::new(),
            claims,
            rechecks: Notify::new(),
            commands,
            snapshot: parking_lot::Mutex::new(SchedulingState {
                ready: false,
                degraded: self.runner.is_degraded(),
                credits: self.hello.credits,
                offers_sent: 0,
                offers_seen: 0,
                ready_revision: None,
                held: Vec::new(),
                last_offer: tokio::time::Instant::now(),
            }),
        }
    }

    /// Only committed ownership is advertised as welcome; headers alone prove no session authority.
    async fn welcome(&mut self) -> Result<()> {
        let welcome = Message::Welcome {
            request_id: self.hello.request_id,
            connection_id: self.authority.connection().as_str().into(),
            heartbeat_interval_ms: opening::interval_millis()?,
        };
        let frame = af_stream::encode(
            &welcome,
            &self.gateway.inner.output_budget,
            StreamKind::Control,
            &self.context.native.stop,
            self.deadline,
        )
        .await?;
        af_stream::write(
            &mut self.io.writer,
            &frame,
            &self.context.native.stop,
            self.deadline,
        )
        .await?;
        Ok(())
    }
}

impl SessionServices {
    pub(crate) fn connected(
        gateway: crate::Gateway,
        handle: Arc<SessionHandle>,
        authority: afd_runner::capacity::authority::Authority,
        cache: super::cache::LeaseCache,
        output: super::StreamWriters,
        accepted: af_wire::lease::AcceptedKinds,
    ) -> Self {
        let dependencies = &gateway.inner.dependencies;
        Self {
            leases: dependencies
                .leases
                .clone()
                .for_connection(authority.clone()),
            drafts: dependencies
                .drafts
                .clone()
                .for_connection(authority.clone()),
            models: dependencies
                .models
                .clone()
                .for_connection(authority.clone()),
            creations: dependencies
                .creations
                .clone()
                .for_connection(authority.clone()),
            authority,
            cache,
            output,
            accepted,
            handle,
            gateway,
        }
    }
}

/// A missing lease is cancelled; an unlisted durable lease is left to lapse rather than replaying secrets.
async fn reconcile(services: &SessionServices, running: &[af_core::id::Uuid7]) -> Result<()> {
    for (lease, code) in services.cache.reconciliation(
        running,
        services.gateway.inner.dependencies.wall_clock.now(),
    ) {
        services.cancel(&lease, code).await?;
    }
    Ok(())
}
