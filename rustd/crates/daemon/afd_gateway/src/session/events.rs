//! Typed event selection is separate from session decisions, following Exonum's handler pattern.

use std::ops::ControlFlow;
use afd_bus::Command;
use af_wire::protocol::StreamKind;
use tokio::{sync::mpsc, task::{JoinError, JoinSet}};
use crate::{Result, error::ErrorKind};
use super::{io::IncomingFrame, streams::StreamReceivers, supervisor::SessionSupervisor};

/// Events carry their domain meaning; command variants are not reduced to an anonymous wake.
pub(super) enum SessionEvent {
    Frame(IncomingFrame),
    Completed(OperationCompletion),
    Command(Command),
    OfferRequested,
    DrainRequested,
    Superseded,
    Disconnected,
}

pub(super) enum OperationCompletion { Request, Offer, Command }

pub(super) enum ShutdownReason { Drain, Superseded, Disconnected }

/// The handler owns state transitions; the event source owns asynchronous selection.
pub(super) trait EventHandler {
    fn handle(&mut self, event: SessionEvent) -> Result<ControlFlow<ShutdownReason>>;
}

pub(super) struct EventSource {
    pub(super) frames: StreamReceivers,
    pub(super) commands: mpsc::Receiver<Command>,
}

impl EventSource {
    /// Control remains independent of data queues; background signals cannot spawn duplicate jobs.
    pub(super) async fn next(
        &mut self, session: &mut SessionSupervisor, workers: &mut JoinSet<Result<()>>,
    ) -> Result<SessionEvent> {
        let handle = &session.services.handle;
        tokio::select! {
            biased;
            () = handle.stop.cancelled() => Ok(SessionEvent::Disconnected),
            () = handle.superseded.cancelled() => Ok(SessionEvent::Superseded),
            () = handle.drain.cancelled() => Ok(SessionEvent::DrainRequested),
            done = session.jobs.join_next(), if !session.jobs.is_empty() => Ok(SessionEvent::Completed(join_result(done)?)),
            done = workers.join_next(), if !workers.is_empty() => { join_result(done)?; Err(ErrorKind::Closed.into()) }
            command = self.commands.recv(), if !session.command_running => {
                Ok(SessionEvent::Command(command.ok_or(ErrorKind::Closed)?))
            }
            () = handle.rechecks.notified(), if !session.command_running => Ok(SessionEvent::Command(Command::Recheck)),
            frame = self.frames.control.recv(), if session.slots.available(StreamKind::Control) => frame_event(frame),
            () = handle.offers.notified(), if !session.offer_running => Ok(SessionEvent::OfferRequested),
            frame = self.frames.reports.recv(), if session.slots.available(StreamKind::Reports) => frame_event(frame),
            frame = self.frames.work.recv(), if session.slots.available(StreamKind::Work) => frame_event(frame),
            frame = self.frames.activity.recv() => frame_event(frame),
        }
    }
}

fn frame_event(frame: Option<IncomingFrame>) -> Result<SessionEvent> {
    frame.map(SessionEvent::Frame).ok_or_else(|| ErrorKind::Closed.into())
}

/// Name the three task exit layers rather than hiding them in chained question marks.
pub(super) fn join_result<T>(joined: Option<std::result::Result<Result<T>, JoinError>>) -> Result<T> {
    let task = joined.ok_or(ErrorKind::Closed)?;
    let operation = task?;
    operation
}
