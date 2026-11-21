//! Independent bounded writers keep report and progress pressure away from control.

use af_stream::OwnedFrame;
use af_wire::protocol::StreamKind;
use tokio::{sync::mpsc, time::Instant};
use tokio_util::sync::CancellationToken;

use crate::{Result, error::ErrorKind};

pub(crate) struct Outbound {
    pub(crate) frame: OwnedFrame,
    pub(crate) deadline: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct Queues {
    priority: mpsc::Sender<Outbound>,
    control: mpsc::Sender<Outbound>,
    work: mpsc::Sender<Outbound>,
    reports: mpsc::Sender<Outbound>,
    progress: mpsc::Sender<Outbound>,
}

pub(crate) struct Receivers {
    pub(crate) priority: mpsc::Receiver<Outbound>,
    pub(crate) control: mpsc::Receiver<Outbound>,
    pub(crate) work: mpsc::Receiver<Outbound>,
    pub(crate) reports: mpsc::Receiver<Outbound>,
    pub(crate) progress: mpsc::Receiver<Outbound>,
}

impl Queues {
    pub(crate) fn new() -> (Self, Receivers) {
        let (priority, priority_rx) = mpsc::channel(64);
        let (control, control_rx) = mpsc::channel(16);
        let (work, work_rx) = mpsc::channel(16);
        let (reports, reports_rx) = mpsc::channel(16);
        let (progress, progress_rx) = mpsc::channel(16);
        (
            Self {
                priority,
                control,
                work,
                reports,
                progress,
            },
            Receivers {
                priority: priority_rx,
                control: control_rx,
                work: work_rx,
                reports: reports_rx,
                progress: progress_rx,
            },
        )
    }

    pub(crate) async fn send(
        &self,
        kind: StreamKind,
        priority: bool,
        message: Outbound,
        stop: &CancellationToken,
    ) -> Result<()> {
        let queue = match kind {
            StreamKind::Control if priority => &self.priority,
            StreamKind::Control => &self.control,
            StreamKind::Work => &self.work,
            StreamKind::Reports => &self.reports,
            StreamKind::Progress => &self.progress,
        };
        let deadline = message.deadline;
        tokio::select! {
            biased;
            () = stop.cancelled() => Err(ErrorKind::Disconnected.into()),
            sent = tokio::time::timeout_at(deadline, queue.send(message)) => {
                sent?.map_err(crate::error::stream_disconnected)
            }
        }
    }

    pub(crate) fn progress(&self, message: Outbound) -> Result<()> {
        match self.progress.try_send(message) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                {
                    let (event, reason) = (&"runner_stream_progress_dropped", &"queue_capacity");
                    tracing::debug!(event, reason);
                };
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(ErrorKind::Disconnected.into()),
        }
    }
}
