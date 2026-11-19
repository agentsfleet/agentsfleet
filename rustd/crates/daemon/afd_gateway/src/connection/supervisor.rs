//! One handler schedules bounded effects; every effect remains in the owner's task set.

use super::{
    ConnectionServices, Message,
    events::{
        ConnectionEvent, EventHandler, EventSource, OperationCompletion, ShutdownReason,
        join_result,
    },
    io::IncomingFrame,
    offers::{OfferMode, OfferOutcome, OfferTrigger},
    requests::{RequestSlots, RequestTracker},
};
use crate::{
    Operation, Result,
    error::{ErrorKind, within},
};
use af_core::{error_code, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::protocol::StreamKind;
use afd_bus::Command;
use std::{ops::ControlFlow, sync::Arc};
use tokio::{task::JoinSet, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

/// Busy flags represent the only two coalesced background operations.
/// Request identity and capacity are instead held by drop-owned guards.
pub(super) struct ConnectionSupervisor {
    pub(super) services: Arc<ConnectionServices>,
    pub(super) jobs: JoinSet<Result<OperationCompletion>>,
    pub(super) slots: RequestSlots,
    pub(super) assign_running: bool,
    pub(super) command_running: bool,
    requests: Arc<RequestTracker>,
    span: tracing::Span,
}

impl ConnectionSupervisor {
    pub(super) fn new(services: Arc<ConnectionServices>) -> Self {
        let runner_id = services.handle.runner.as_str();
        let connection_id = services.handle.connection_id.as_str();
        let span = tracing::debug_span!("runner_connection", runner_id, connection_id);
        Self {
            services,
            jobs: JoinSet::new(),
            slots: RequestSlots::default(),
            assign_running: false,
            command_running: false,
            requests: Arc::new(RequestTracker::default()),
            span,
        }
    }

    /// Like Exonum's handler, the owner consumes one typed event and one transition at a time.
    pub(super) async fn run(
        &mut self,
        events: &mut EventSource,
        workers: &mut JoinSet<Result<()>>,
        read_stop: &CancellationToken,
    ) -> Result<()> {
        let reason = loop {
            let event = events.next(self, workers).await?;
            if let ControlFlow::Break(reason) = self.handle(event)? {
                break reason;
            }
        };
        self.close(reason, read_stop, &mut events.frames).await
    }

    fn request(&mut self, frame: IncomingFrame) -> Result<()> {
        let Some(permit) = self.slots.admit(frame.kind) else {
            if frame.kind == StreamKind::Activity {
                tracing::debug!(
                    event = "runner_stream_activity_dropped",
                    reason = "worker_capacity"
                );
                return Ok(());
            }
            return Err(ErrorKind::Protocol.into());
        };
        let services = Arc::clone(&self.services);
        let requests = Arc::clone(&self.requests);
        let work = frame.kind == StreamKind::Work;
        let gateway = services.gateway.clone();
        let request = async move {
            let _permit = permit;
            services.dispatch(frame, &requests).await?;
            Ok(OperationCompletion::Request)
        }
        .instrument(self.span.clone());
        if work {
            // A physical provider call must finish its receipt after the connection closes.
            // The tracked request retains its frame charge, slot and request registration.
            let owner = gateway.inner.tasks.spawn(request);
            self.jobs.spawn(async move { owner.await? });
        } else {
            self.jobs.spawn(request);
        }
        Ok(())
    }

    fn offer(&mut self, trigger: OfferTrigger) {
        self.assign_running = true;
        let services = Arc::clone(&self.services);
        self.jobs.spawn(
            async move {
                let mode = match &trigger {
                    OfferTrigger::Ready => OfferMode::Ordinary,
                    OfferTrigger::Pass(request) if !request.reply.is_closed() => request.mode,
                    OfferTrigger::Pass(_) => return Ok(OperationCompletion::Assign),
                };
                let outcome = if mode == OfferMode::Ordinary
                    && services.handle.snapshot.lock().recovery_pending.is_some()
                {
                    // Only the replica's bounded drain issues recovery claims.
                    services.gateway.inner.dependencies.wake.recheck();
                    OfferOutcome::Deferred
                } else {
                    services.offer(mode).await?
                };
                match trigger {
                    OfferTrigger::Ready if outcome == OfferOutcome::Issued => {
                        services.handle.request_offer();
                    }
                    OfferTrigger::Ready => {}
                    OfferTrigger::Pass(request) => {
                        if request.reply.send(outcome).is_err() {
                            tracing::debug!(event = "runner_offer_pass_abandoned", ?outcome);
                        }
                    }
                }
                Ok(OperationCompletion::Assign)
            }
            .instrument(self.span.clone()),
        );
    }

    fn command(&mut self, command: Command) {
        self.command_running = true;
        let services = Arc::clone(&self.services);
        self.jobs.spawn(
            async move {
                if let Err(error) = services.command(command).await
                    && !error.retryable()
                {
                    services
                        .send(&Message::GoAway {
                            error_code: Some(error.code().as_str().into()),
                        })
                        .await?;
                    services.handle.stop.cancel();
                }
                Ok(OperationCompletion::Command)
            }
            .instrument(self.span.clone()),
        );
    }

    fn completed(&mut self, completion: OperationCompletion) {
        match completion {
            OperationCompletion::Request => {}
            OperationCompletion::Assign => self.assign_running = false,
            OperationCompletion::Command => self.command_running = false,
        }
    }

    async fn close(
        &mut self,
        reason: ShutdownReason,
        read_stop: &CancellationToken,
        frames: &mut super::streams::StreamReceivers,
    ) -> Result<()> {
        match reason {
            ShutdownReason::Disconnected => Ok(()),
            ShutdownReason::Silent => {
                tracing::info!(event = "runner_stream_completed", reason = "silent");
                Ok(())
            }
            ShutdownReason::Superseded => {
                self.services
                    .send(&Message::GoAway {
                        error_code: Some(error_code::RUN_STREAM_SUPERSEDED.as_str().into()),
                    })
                    .await
            }
            ShutdownReason::Drain => self.drain(read_stop, frames).await,
        }
    }

    /// Stop taking frames, observe accepted operations and their flushed answers, then send `go_away`.
    async fn drain(
        &mut self,
        read_stop: &CancellationToken,
        frames: &mut super::streams::StreamReceivers,
    ) -> Result<()> {
        read_stop.cancel();
        let stop = self.services.handle.stop.clone();
        within(
            &stop,
            Instant::now() + STREAM_ANSWER_TIMEOUT,
            Operation::Drain,
            async {
                while !frames.drained() || !self.jobs.is_empty() {
                    tokio::select! {
                        biased;
                        done = self.jobs.join_next(), if !self.jobs.is_empty() => {
                            self.completed(join_result(done)?);
                        }
                        frame = frames.next(&self.slots), if !frames.drained() => {
                            if let Some(frame) = frame {
                                self.request(frame)?;
                            }
                        }
                    }
                }
                self.services
                    .send(&Message::GoAway { error_code: None })
                    .await
            },
        )
        .await
    }
}

impl EventHandler for ConnectionSupervisor {
    fn handle(&mut self, event: ConnectionEvent) -> Result<ControlFlow<ShutdownReason>> {
        match event {
            ConnectionEvent::Frame(frame) => self.request(frame)?,
            ConnectionEvent::Completed(completion) => self.completed(completion),
            ConnectionEvent::Command(command) => self.command(command),
            ConnectionEvent::OfferRequested(trigger) => self.offer(trigger),
            ConnectionEvent::DrainRequested => {
                return Ok(ControlFlow::Break(ShutdownReason::Drain));
            }
            ConnectionEvent::Superseded => {
                return Ok(ControlFlow::Break(ShutdownReason::Superseded));
            }
            ConnectionEvent::Disconnected => {
                return Ok(ControlFlow::Break(ShutdownReason::Disconnected));
            }
            ConnectionEvent::Silent => return Ok(ControlFlow::Break(ShutdownReason::Silent)),
        }
        Ok(ControlFlow::Continue(()))
    }
}
