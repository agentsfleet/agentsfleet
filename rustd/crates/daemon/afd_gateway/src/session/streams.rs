//! Each stream keeps independent backpressure; the assembly owns every reader and writer task.

use super::io::{self, IncomingFrame, StreamAttachment, StreamIo, StreamWriters, WriteQueue};
use crate::{
    Operation, Result,
    error::{ErrorKind, within},
};
use af_stream::Budget;
use af_wire::protocol::StreamKind;
use std::collections::BTreeMap;
use tokio::{sync::mpsc, task::JoinSet, time::Instant};
use tokio_util::sync::CancellationToken;

/// Bounded input queues are separate even though the owner consumes typed events from them.
pub(super) struct StreamReceivers {
    pub(super) control: mpsc::Receiver<IncomingFrame>,
    pub(super) work: mpsc::Receiver<IncomingFrame>,
    pub(super) reports: mpsc::Receiver<IncomingFrame>,
    pub(super) activity: mpsc::Receiver<IncomingFrame>,
}

impl StreamReceivers {
    /// Readers release their senders after handing off their last fully read frame.
    pub(super) fn drained(&self) -> bool {
        [&self.control, &self.work, &self.reports, &self.activity]
            .iter()
            .all(|queue| queue.is_closed() && queue.is_empty())
    }

    /// Preserve operation limits while consuming the finite input left by stopped readers.
    pub(super) async fn next(
        &mut self,
        slots: &super::requests::RequestSlots,
    ) -> Option<IncomingFrame> {
        tokio::select! {
            biased;
            frame = self.control.recv(), if pending(&self.control) && slots.available(StreamKind::Control) => frame,
            frame = self.reports.recv(), if pending(&self.reports) && slots.available(StreamKind::Reports) => frame,
            frame = self.work.recv(), if pending(&self.work) && slots.available(StreamKind::Work) => frame,
            frame = self.activity.recv(), if pending(&self.activity) && slots.available(StreamKind::Activity) => frame,
            else => std::future::pending().await,
        }
    }
}

fn pending(queue: &mpsc::Receiver<IncomingFrame>) -> bool {
    !queue.is_closed() || !queue.is_empty()
}

struct StreamSenders {
    control: mpsc::Sender<IncomingFrame>,
    work: mpsc::Sender<IncomingFrame>,
    reports: mpsc::Sender<IncomingFrame>,
    activity: mpsc::Sender<IncomingFrame>,
}

impl StreamSenders {
    fn sender(&self, kind: StreamKind) -> &mpsc::Sender<IncomingFrame> {
        match kind {
            StreamKind::Control => &self.control,
            StreamKind::Work => &self.work,
            StreamKind::Reports => &self.reports,
            StreamKind::Activity => &self.activity,
        }
    }
}

/// Settings and cancellation belong to one physical connection.
pub(super) struct StreamSetup {
    pub(super) budget: Budget,
    pub(super) stop: CancellationToken,
    pub(super) deadline: Instant,
}

pub(super) struct SessionStreams {
    pub(super) frames: StreamReceivers,
    pub(super) writers: StreamWriters,
    pub(super) workers: JoinSet<Result<()>>,
    pub(super) read_stop: CancellationToken,
}

struct StreamAssembly {
    setup: StreamSetup,
    senders: StreamSenders,
    receivers: StreamReceivers,
    writers: BTreeMap<StreamKind, WriteQueue>,
    workers: JoinSet<Result<()>>,
    read_stop: CancellationToken,
}

impl SessionStreams {
    /// Three secondary requests must attach before the common opening deadline.
    pub(super) async fn attach(
        control: StreamIo,
        mut attachments: mpsc::Receiver<StreamAttachment>,
        setup: StreamSetup,
    ) -> Result<Self> {
        let mut assembly = StreamAssembly::new(setup);
        assembly.add(control)?;
        for _ in 0..3 {
            let stream = within(
                &assembly.setup.stop,
                assembly.setup.deadline,
                Operation::Opening,
                async {
                    attachments
                        .recv()
                        .await
                        .ok_or_else(|| ErrorKind::Closed.into())
                },
            )
            .await?;
            let stream = stream.into_io(assembly.setup.budget.clone(), assembly.read_stop.clone());
            assembly.add(stream)?;
        }
        Ok(Self {
            frames: assembly.receivers,
            writers: assembly.writers.try_into()?,
            workers: assembly.workers,
            read_stop: assembly.read_stop,
        })
    }
}

impl StreamAssembly {
    fn new(setup: StreamSetup) -> Self {
        let (control, control_rx) = mpsc::channel(8);
        let (work, work_rx) = mpsc::channel(8);
        let (reports, reports_rx) = mpsc::channel(8);
        let (activity, activity_rx) = mpsc::channel(8);
        Self {
            read_stop: setup.stop.child_token(),
            setup,
            workers: JoinSet::new(),
            writers: BTreeMap::new(),
            senders: StreamSenders {
                control,
                work,
                reports,
                activity,
            },
            receivers: StreamReceivers {
                control: control_rx,
                work: work_rx,
                reports: reports_rx,
                activity: activity_rx,
            },
        }
    }

    /// One insertion owns both tasks; duplicate attachment cannot replace an existing writer.
    fn add(&mut self, stream: StreamIo) -> Result<()> {
        if self.writers.contains_key(&stream.kind) {
            return Err(ErrorKind::DuplicateStream { kind: stream.kind }.into());
        }
        self.workers.spawn(io::read_stream(
            stream.reader,
            stream.kind,
            self.senders.sender(stream.kind).clone(),
            self.read_stop.clone(),
        ));
        let writer = io::writer(stream.writer, self.setup.stop.clone(), &mut self.workers);
        self.writers.insert(stream.kind, writer);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Drain consumes completed reads while preserving the independent operation limits.

    use super::StreamReceivers;
    use crate::session::{io::IncomingFrame, requests::RequestSlots};
    use af_stream::Budget;
    use af_wire::protocol::StreamKind;
    use serde_json::json;
    use tokio::{sync::mpsc, time::Instant};
    use tokio_util::sync::CancellationToken;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    #[tokio::test]
    async fn stopped_readers_leave_every_queued_frame_for_drain() -> TestResult {
        let mut queues = filled_queues().await?;
        let slots = RequestSlots::default();
        let mut control_jobs = Vec::new();
        while let Some(permit) = slots.admit(StreamKind::Control) {
            control_jobs.push(permit);
        }
        assert!(!queues.drained());
        let mut seen = Vec::new();
        for _ in 0..3 {
            let frame = queues.next(&slots).await.ok_or("queued frame missing")?;
            assert_ne!(
                frame.kind,
                StreamKind::Control,
                "a full stream must retain its queued frame"
            );
            seen.push(frame.kind);
        }
        assert_eq!(
            seen,
            [StreamKind::Reports, StreamKind::Work, StreamKind::Activity]
        );
        assert!(!queues.drained(), "control still owns one accepted frame");
        drop(control_jobs);
        let frame = queues
            .next(&slots)
            .await
            .ok_or("accepted control frame lost")?;
        assert_eq!(frame.kind, StreamKind::Control);
        assert_eq!(
            frame.frame.decode::<serde_json::Value>()?,
            json!({"kind":"control"})
        );
        assert!(queues.drained());
        Ok(())
    }

    async fn filled_queues() -> Result<StreamReceivers, Box<dyn std::error::Error + Send + Sync>> {
        let (control, control_rx) = mpsc::channel(2);
        let (reports, reports_rx) = mpsc::channel(2);
        let (work, work_rx) = mpsc::channel(2);
        let (activity, activity_rx) = mpsc::channel(2);
        let budget = Budget::default();
        let stop = CancellationToken::new();
        for (kind, sender) in [
            (StreamKind::Control, &control),
            (StreamKind::Reports, &reports),
            (StreamKind::Work, &work),
            (StreamKind::Activity, &activity),
        ] {
            let frame = af_stream::encode(
                &json!({"kind":kind.as_str()}),
                &budget,
                kind,
                &stop,
                Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
            )
            .await?;
            sender.send(IncomingFrame { kind, frame }).await?;
        }
        drop((control, reports, work, activity));
        Ok(StreamReceivers {
            control: control_rx,
            reports: reports_rx,
            work: work_rx,
            activity: activity_rx,
        })
    }
}
