//! Provisional and committed connection states each have one small, explicit ownership transition.

use super::{
    ConfirmedConnection, ConnectionHandle, ConnectionOpening, ConnectionServices, Message,
    SchedulingState, commands,
    events::EventSource,
    opening,
    streams::{ConnectionStreams, StreamSetup},
    supervisor::ConnectionSupervisor,
};
use crate::Result;
use af_wire::protocol::{PENDING_MAX, StreamKind};
use std::sync::Arc;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

impl ConnectionOpening {
    /// Any failure or dropped opening cancels all native streams, including a partly read hello.
    pub(super) async fn run(self) -> Result<()> {
        let _cancel_on_exit = self.context.native.stop.clone().drop_guard();
        self.confirm().await?.run().await
    }
}

impl ConfirmedConnection {
    /// Registration and the admission permit live until every supervised task has joined.
    async fn run(mut self) -> Result<()> {
        let (commands_tx, commands_rx) = mpsc::channel(PENDING_MAX);
        let (passes_tx, passes_rx) = mpsc::channel(1);
        let handle = Arc::new(self.handle(commands_tx, passes_tx));
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
        let mut streams = ConnectionStreams::attach(self.io, self.attachments, setup).await?;
        let services = Arc::new(ConnectionServices::connected(
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
        let mut supervisor = ConnectionSupervisor::new(services);
        let mut events = EventSource {
            frames: streams.frames,
            commands: commands_rx,
            passes: passes_rx,
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
        passes: mpsc::Sender<super::PassRequest>,
    ) -> ConnectionHandle {
        ConnectionHandle {
            runner: self.runner.id().clone(),
            connection_id: self.authority.connection().clone(),
            stop: self.context.native.stop.clone(),
            drain: CancellationToken::new(),
            liveness: super::liveness::Liveness::new(
                self.gateway.inner.dependencies.runners.heartbeat_interval(),
            ),
            superseded: CancellationToken::new(),
            assignments: Notify::new(),
            passes,
            recovery_scan: parking_lot::Mutex::default(),
            rechecks: Notify::new(),
            commands,
            snapshot: parking_lot::Mutex::new(SchedulingState {
                ready: false,
                degraded: self.runner.is_degraded(),
                credits: self.hello.credits,
                assignments_sent: 0,
                assignments_seen: 0,
                ready_revision: None,
                held: Vec::new(),
                last_assignment: tokio::time::Instant::now(),
                recovery_pending: None,
            }),
        }
    }

    /// Only committed ownership is advertised as welcome; headers alone prove no connection authority.
    async fn welcome(&mut self) -> Result<()> {
        let welcome = Message::Welcome {
            request_id: self.hello.request_id,
            connection_id: self.authority.connection().as_str().into(),
            heartbeat_interval_ms: opening::interval_millis(
                &self.gateway.inner.dependencies.runners,
            )?,
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

impl ConnectionServices {
    pub(crate) fn connected(
        gateway: crate::Gateway,
        handle: Arc<ConnectionHandle>,
        authority: afd_runner::capacity::authority::Authority,
        cache: super::cache::RunCache,
        output: super::StreamWriters,
        accepted: af_wire::run::AcceptedKinds,
    ) -> Self {
        let dependencies = &gateway.inner.dependencies;
        Self {
            runs: dependencies.runs.clone().for_connection(authority.clone()),
            drafts: dependencies.drafts.clone(),
            creations: dependencies.creations.clone(),
            authority,
            cache,
            output,
            accepted,
            checkpoint_verifications: tokio::sync::Semaphore::new(2),
            renewal_ticks: std::sync::atomic::AtomicU64::new(0),
            handle,
            gateway,
        }
    }
}

/// A missing run is cancelled; an unlisted durable run is left to lapse rather than replaying secrets.
async fn reconcile(services: &ConnectionServices, running: &[af_core::id::Uuid7]) -> Result<()> {
    for (run, code) in services.cache.reconciliation(
        running,
        services.gateway.inner.dependencies.wall_clock.now(),
    ) {
        services.cancel(&run, code).await?;
    }
    Ok(())
}
