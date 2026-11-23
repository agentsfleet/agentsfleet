//! Typed event selection is separate from connection decisions, following Exonum's handler pattern.

use super::assignments::{AssignTrigger, PassRequest};
use super::{io::IncomingFrame, streams::StreamReceivers, supervisor::ConnectionSupervisor};
use crate::{Result, error::ErrorKind};
use af_wire::protocol::StreamKind;
use afd_bus::Command;
use std::ops::ControlFlow;
use tokio::{
    sync::mpsc,
    task::{JoinError, JoinSet},
};

/// Events carry their domain meaning; command variants are not reduced to an anonymous wake.
pub(super) enum ConnectionEvent {
    Frame(IncomingFrame),
    Completed(OperationCompletion),
    Command(Command),
    AssignRequested(AssignTrigger),
    DrainRequested,
    Superseded,
    Disconnected,
    Silent,
}

#[derive(Clone, Copy)]
pub(super) enum OperationCompletion {
    Request,
    Assign,
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
    fn handle(&mut self, event: ConnectionEvent) -> Result<ControlFlow<ShutdownReason>>;
}

pub(super) struct EventSource {
    pub(super) frames: StreamReceivers,
    pub(super) commands: mpsc::Receiver<Command>,
    pub(super) passes: mpsc::Receiver<PassRequest>,
}

impl EventSource {
    /// Control remains independent of data queues; background signals cannot spawn duplicate jobs.
    pub(super) async fn next(
        &mut self,
        connection: &mut ConnectionSupervisor,
        workers: &mut JoinSet<Result<()>>,
    ) -> Result<ConnectionEvent> {
        let handle = &connection.services.handle;
        tokio::select! {
            biased;
            () = handle.stop.cancelled() => Ok(ConnectionEvent::Disconnected),
            () = handle.superseded.cancelled() => Ok(ConnectionEvent::Superseded),
            () = handle.drain.cancelled() => Ok(ConnectionEvent::DrainRequested),
            () = handle.liveness.silence() => Ok(ConnectionEvent::Silent),
            done = connection.jobs.join_next(), if !connection.jobs.is_empty() => Ok(ConnectionEvent::Completed(join_result(done)?)),
            done = workers.join_next(), if !workers.is_empty() => { join_result(done)?; Err(ErrorKind::Closed.into()) }
            command = self.commands.recv(), if !connection.command_running => {
                Ok(ConnectionEvent::Command(command.ok_or(ErrorKind::Closed)?))
            }
            () = handle.rechecks.notified(), if !connection.command_running => Ok(ConnectionEvent::Command(Command::Recheck)),
            frame = self.frames.control.recv(), if connection.slots.available(StreamKind::Control) => frame_event(frame),
            pass = self.passes.recv(), if !connection.assign_running => {
                Ok(ConnectionEvent::AssignRequested(AssignTrigger::Pass(pass.ok_or(ErrorKind::Closed)?)))
            }
            () = handle.assignments.notified(), if !connection.assign_running => Ok(ConnectionEvent::AssignRequested(AssignTrigger::Ready)),
            frame = self.frames.reports.recv(), if connection.slots.available(StreamKind::Reports) => frame_event(frame),
            frame = self.frames.work.recv(), if connection.slots.available(StreamKind::Work) => frame_event(frame),
            frame = self.frames.progress.recv() => frame_event(frame),
        }
    }
}

fn frame_event(frame: Option<IncomingFrame>) -> Result<ConnectionEvent> {
    frame
        .map(ConnectionEvent::Frame)
        .ok_or_else(|| ErrorKind::Closed.into())
}

/// Name the three task exit layers rather than hiding them in chained question marks.
pub(super) fn join_result<T>(
    joined: Option<std::result::Result<Result<T>, JoinError>>,
) -> Result<T> {
    let task = joined.ok_or(ErrorKind::Closed)?;
    task?
}
