//! Typed event selection is separate from session decisions, following Exonum's handler pattern.

use super::offers::OfferTrigger;
use super::{io::IncomingFrame, streams::StreamReceivers, supervisor::SessionSupervisor};
use crate::{Result, error::ErrorKind};
use af_wire::protocol::StreamKind;
use afd_bus::Command;
use std::ops::ControlFlow;
use tokio::{
    sync::{mpsc, oneshot},
    task::{JoinError, JoinSet},
};

/// Events carry their domain meaning; command variants are not reduced to an anonymous wake.
pub(super) enum SessionEvent {
    Frame(IncomingFrame),
    Completed(OperationCompletion),
    Command(Command),
    OfferRequested(OfferTrigger),
    DrainRequested,
    Superseded,
    Disconnected,
    Silent,
}

#[derive(Clone, Copy)]
pub(super) enum OperationCompletion {
    Request,
    Offer,
    Command,
}

pub(super) enum ShutdownReason {
    Drain,
    Superseded,
    Disconnected,
    Silent,
}

/// The handler owns state transitions; the event source owns asynchronous selection.
pub(super) trait EventHandler {
    fn handle(&mut self, event: SessionEvent) -> Result<ControlFlow<ShutdownReason>>;
}

pub(super) struct EventSource {
    pub(super) frames: StreamReceivers,
    pub(super) commands: mpsc::Receiver<Command>,
    pub(super) claims: mpsc::Receiver<oneshot::Sender<bool>>,
}

impl EventSource {
    /// Control remains independent of data queues; background signals cannot spawn duplicate jobs.
    pub(super) async fn next(
        &mut self,
        session: &mut SessionSupervisor,
        workers: &mut JoinSet<Result<()>>,
    ) -> Result<SessionEvent> {
        let handle = &session.services.handle;
        tokio::select! {
            biased;
            () = handle.stop.cancelled() => Ok(SessionEvent::Disconnected),
            () = handle.superseded.cancelled() => Ok(SessionEvent::Superseded),
            () = handle.drain.cancelled() => Ok(SessionEvent::DrainRequested),
            () = handle.liveness.silence() => Ok(SessionEvent::Silent),
            done = session.jobs.join_next(), if !session.jobs.is_empty() => Ok(SessionEvent::Completed(join_result(done)?)),
            done = workers.join_next(), if !workers.is_empty() => { join_result(done)?; Err(ErrorKind::Closed.into()) }
            command = self.commands.recv(), if !session.command_running => {
                Ok(SessionEvent::Command(command.ok_or(ErrorKind::Closed)?))
            }
            () = handle.rechecks.notified(), if !session.command_running => Ok(SessionEvent::Command(Command::Recheck)),
            frame = self.frames.control.recv(), if session.slots.available(StreamKind::Control) => frame_event(frame),
            claim = self.claims.recv(), if !session.offer_running => {
                Ok(SessionEvent::OfferRequested(OfferTrigger::Pass(claim.ok_or(ErrorKind::Closed)?)))
            }
            () = handle.offers.notified(), if !session.offer_running => Ok(SessionEvent::OfferRequested(OfferTrigger::Ready)),
            frame = self.frames.reports.recv(), if session.slots.available(StreamKind::Reports) => frame_event(frame),
            frame = self.frames.work.recv(), if session.slots.available(StreamKind::Work) => frame_event(frame),
            frame = self.frames.activity.recv() => frame_event(frame),
        }
    }
}

fn frame_event(frame: Option<IncomingFrame>) -> Result<SessionEvent> {
    frame
        .map(SessionEvent::Frame)
        .ok_or_else(|| ErrorKind::Closed.into())
}

/// Name the three task exit layers rather than hiding them in chained question marks.
pub(super) fn join_result<T>(
    joined: Option<std::result::Result<Result<T>, JoinError>>,
) -> Result<T> {
    let task = joined.ok_or(ErrorKind::Closed)?;
    task?
}
