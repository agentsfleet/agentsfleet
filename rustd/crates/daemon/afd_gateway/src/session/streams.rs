//! Each stream keeps independent backpressure; the assembly owns every reader and writer task.

use std::collections::BTreeMap;
use af_stream::Budget;
use af_wire::protocol::StreamKind;
use tokio::{sync::mpsc, task::JoinSet, time::Instant};
use tokio_util::sync::CancellationToken;
use crate::{Operation, Result, error::{ErrorKind, within}};
use super::io::{self, IncomingFrame, StreamAttachment, StreamIo, StreamWriters, WriteQueue};

/// Bounded input queues are separate even though the owner consumes typed events from them.
pub(super) struct StreamReceivers {
    pub(super) control: mpsc::Receiver<IncomingFrame>,
    pub(super) work: mpsc::Receiver<IncomingFrame>,
    pub(super) reports: mpsc::Receiver<IncomingFrame>,
    pub(super) activity: mpsc::Receiver<IncomingFrame>,
}

struct StreamSenders {
    control: mpsc::Sender<IncomingFrame>, work: mpsc::Sender<IncomingFrame>,
    reports: mpsc::Sender<IncomingFrame>, activity: mpsc::Sender<IncomingFrame>,
}

impl StreamSenders {
    fn sender(&self, kind: StreamKind) -> &mpsc::Sender<IncomingFrame> {
        match kind { StreamKind::Control => &self.control, StreamKind::Work => &self.work,
            StreamKind::Reports => &self.reports, StreamKind::Activity => &self.activity }
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
    pub(super) async fn attach(control: StreamIo, mut attachments: mpsc::Receiver<StreamAttachment>,
        setup: StreamSetup) -> Result<Self> {
        let mut assembly = StreamAssembly::new(setup);
        assembly.add(control)?;
        for _ in 0..3 {
            let stream = within(&assembly.setup.stop, assembly.setup.deadline, Operation::Opening, async {
                attachments.recv().await.ok_or_else(|| ErrorKind::Closed.into())
            }).await?;
            let stream = stream.into_io(assembly.setup.budget.clone(), assembly.read_stop.clone());
            assembly.add(stream)?;
        }
        Ok(Self { frames: assembly.receivers, writers: assembly.writers.try_into()?,
            workers: assembly.workers, read_stop: assembly.read_stop })
    }
}

impl StreamAssembly {
    fn new(setup: StreamSetup) -> Self {
        let (control, control_rx) = mpsc::channel(8);
        let (work, work_rx) = mpsc::channel(8);
        let (reports, reports_rx) = mpsc::channel(8);
        let (activity, activity_rx) = mpsc::channel(8);
        Self { read_stop: setup.stop.child_token(), setup, workers: JoinSet::new(), writers: BTreeMap::new(),
            senders: StreamSenders { control, work, reports, activity },
            receivers: StreamReceivers { control: control_rx, work: work_rx, reports: reports_rx, activity: activity_rx } }
    }

    /// One insertion owns both tasks; duplicate attachment cannot replace an existing writer.
    fn add(&mut self, stream: StreamIo) -> Result<()> {
        if self.writers.contains_key(&stream.kind) { return Err(ErrorKind::DuplicateStream { kind: stream.kind }.into()); }
        self.workers.spawn(io::read_stream(stream.reader, stream.kind,
            self.senders.sender(stream.kind).clone(), self.read_stop.clone()));
        let writer = io::writer(stream.writer, self.setup.stop.clone(), &mut self.workers);
        self.writers.insert(stream.kind, writer);
        Ok(())
    }
}
