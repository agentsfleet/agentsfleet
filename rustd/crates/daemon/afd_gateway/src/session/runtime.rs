//! Provisional and committed session states each have one small, explicit ownership transition.

use std::sync::Arc;
use af_wire::protocol::{PENDING_MAX, StreamKind};
use tokio::sync::{mpsc, watch, Notify};
use tokio_util::sync::CancellationToken;
use crate::Result;
use super::{ConfirmedSession, Message, SchedulingState, SessionHandle, SessionOpening, SessionServices,
    commands, events::EventSource, opening, streams::{SessionStreams, StreamSetup}, supervisor::SessionSupervisor};

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
        let handle = Arc::new(self.handle(commands_tx));
        let _registration = self.gateway.0.registry.register(Arc::clone(&handle))?;
        if let Err(error) = commands::announce(&self.gateway, &handle.runner).await {
            // Ownership already committed; heartbeat and grant checks still fence the previous process.
            self.gateway.observe(&error, crate::Operation::Recheck);
        }
        self.welcome().await?;
        let setup = StreamSetup { budget: self.gateway.0.input_budget.clone(),
            stop: handle.stop.clone(), deadline: self.deadline };
        let mut streams = SessionStreams::attach(self.io, self.attachments, setup).await?;
        let services = Arc::new(SessionServices {
            gateway: self.gateway.clone(), handle: Arc::clone(&handle),
            leases: self.gateway.0.dependencies.leases.clone().for_connection(self.authority.clone()),
            drafts: self.gateway.0.dependencies.drafts.clone().for_connection(self.authority.clone()),
            authority: self.authority, cache: self.cache, output: streams.writers,
            accepted: self.hello.accepted,
        });
        reconcile(&services, &self.hello.running).await?;
        streams.workers.spawn(commands::listen(Arc::clone(&services)));
        let mut supervisor = SessionSupervisor::new(services);
        let mut events = EventSource { frames: streams.frames, commands: commands_rx };
        let result = supervisor.run(&mut events, &mut streams.workers, &streams.read_stop).await;
        handle.stop.cancel();
        supervisor.jobs.shutdown().await;
        streams.workers.shutdown().await;
        drop(self.permit);
        result
    }

    fn handle(&self, commands: mpsc::Sender<afd_bus::Command>) -> SessionHandle {
        SessionHandle {
            runner: self.runner.id().clone(), connection: self.authority.connection().clone(),
            stop: self.context.native.stop.clone(), drain: CancellationToken::new(),
            superseded: CancellationToken::new(), offers: Notify::new(), rechecks: Notify::new(), commands,
            snapshot: watch::Sender::new(SchedulingState { ready: false, degraded: self.runner.is_degraded(),
                credits: self.hello.credits, held: Vec::new(), last_offer: tokio::time::Instant::now() }),
        }
    }

    /// Only committed ownership is advertised as welcome; headers alone prove no session authority.
    async fn welcome(&mut self) -> Result<()> {
        let welcome = Message::Welcome { request_id: self.hello.request_id,
            connection_id: self.authority.connection().as_str().into(), heartbeat_interval_ms: opening::interval_millis()? };
        let frame = af_stream::encode(&welcome, &self.gateway.0.output_budget, StreamKind::Control,
            &self.context.native.stop, self.deadline).await?;
        af_stream::write(&mut self.io.writer, &frame, &self.context.native.stop, self.deadline).await?;
        Ok(())
    }
}

/// A missing lease is cancelled; an unlisted durable lease is left to lapse rather than replaying secrets.
async fn reconcile(services: &SessionServices, running: &[af_core::id::Uuid7]) -> Result<()> {
    for (lease, code) in services.cache.reconciliation(running, services.gateway.0.dependencies.wall_clock.now()) {
        services.cancel(&lease, code).await?;
    }
    Ok(())
}
