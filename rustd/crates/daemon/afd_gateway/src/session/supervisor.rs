//! One handler schedules bounded effects; every effect remains in the owner's task set.

use super::{
    Message, SessionServices,
    events::{
        EventHandler, EventSource, OperationCompletion, SessionEvent, ShutdownReason, join_result,
    },
    io::IncomingFrame,
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

/// Busy flags represent the only two coalesced background operations.
/// Request identity and capacity are instead held by drop-owned guards.
pub(super) struct SessionSupervisor {
    pub(super) services: Arc<SessionServices>,
    pub(super) jobs: JoinSet<Result<OperationCompletion>>,
    pub(super) slots: RequestSlots,
    pub(super) offer_running: bool,
    pub(super) command_running: bool,
    requests: Arc<RequestTracker>,
}

impl SessionSupervisor {
    pub(super) fn new(services: Arc<SessionServices>) -> Self {
        Self {
            services,
            jobs: JoinSet::new(),
            slots: RequestSlots::default(),
            offer_running: false,
            command_running: false,
            requests: Arc::new(RequestTracker::default()),
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
        self.close(reason, read_stop).await
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
        };
        if work {
            // A physical provider call must finish its receipt after the session closes.
            // The tracked request retains its frame charge, slot and request registration.
            let owner = gateway.inner.tasks.spawn(request);
            self.jobs.spawn(async move { owner.await? });
        } else {
            self.jobs.spawn(request);
        }
        Ok(())
    }

    fn offer(&mut self) {
        self.offer_running = true;
        let services = Arc::clone(&self.services);
        self.jobs.spawn(async move {
            services.offer().await?;
            Ok(OperationCompletion::Offer)
        });
    }

    fn command(&mut self, command: Command) {
        self.command_running = true;
        let services = Arc::clone(&self.services);
        self.jobs.spawn(async move {
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
        });
    }

    fn completed(&mut self, completion: OperationCompletion) {
        match completion {
            OperationCompletion::Request => {}
            OperationCompletion::Offer => self.offer_running = false,
            OperationCompletion::Command => self.command_running = false,
        }
    }

    async fn close(&mut self, reason: ShutdownReason, read_stop: &CancellationToken) -> Result<()> {
        match reason {
            ShutdownReason::Disconnected => Ok(()),
            ShutdownReason::Superseded => {
                self.services
                    .send(&Message::GoAway {
                        error_code: Some(error_code::RUN_STREAM_SUPERSEDED.as_str().into()),
                    })
                    .await
            }
            ShutdownReason::Drain => self.drain(read_stop).await,
        }
    }

    /// Stop taking frames, observe accepted operations and their flushed answers, then send `go_away`.
    async fn drain(&mut self, read_stop: &CancellationToken) -> Result<()> {
        read_stop.cancel();
        let stop = self.services.handle.stop.clone();
        within(
            &stop,
            Instant::now() + STREAM_ANSWER_TIMEOUT,
            Operation::Drain,
            async {
                while let Some(done) = self.jobs.join_next().await {
                    self.completed(join_result(Some(done))?);
                }
                self.services
                    .send(&Message::GoAway { error_code: None })
                    .await
            },
        )
        .await
    }
}

impl EventHandler for SessionSupervisor {
    fn handle(&mut self, event: SessionEvent) -> Result<ControlFlow<ShutdownReason>> {
        match event {
            SessionEvent::Frame(frame) => self.request(frame)?,
            SessionEvent::Completed(completion) => self.completed(completion),
            SessionEvent::Command(command) => self.command(command),
            SessionEvent::OfferRequested => self.offer(),
            SessionEvent::DrainRequested => return Ok(ControlFlow::Break(ShutdownReason::Drain)),
            SessionEvent::Superseded => return Ok(ControlFlow::Break(ShutdownReason::Superseded)),
            SessionEvent::Disconnected => {
                return Ok(ControlFlow::Break(ShutdownReason::Disconnected));
            }
        }
        Ok(ControlFlow::Continue(()))
    }
}
