//! Stream attachment consumes its body once. All pipes and writes stay bounded and owned.

use crate::{
    Operation, Result,
    error::{ErrorKind, within},
};
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::{Budget, OwnedFrame, Reader};
use af_wire::protocol::StreamKind;
use axum::body::Body;
use futures_util::TryStreamExt as _;
use http_body_util::BodyDataStream;
use std::{collections::BTreeMap, pin::Pin};
use tokio::{
    io::{AsyncRead, DuplexStream},
    sync::{mpsc, oneshot},
    task::JoinSet,
    time::Instant,
};
use tokio_util::{
    io::{ReaderStream, StreamReader},
    sync::CancellationToken,
};

/// Item bounds supplement the shared byte budgets; they are not runner worker limits.
const STREAM_QUEUE_DEPTH: usize = 8;
pub(super) type FrameReader = Reader<Pin<Box<dyn AsyncRead + Send>>>;
pub(crate) type StreamAttachmentSender = mpsc::Sender<StreamAttachment>;
pub(super) type WriteQueue = mpsc::Sender<PendingWrite>;

/// A request owns its input body and the write half of its streaming response.
pub(crate) struct StreamAttachment {
    kind: StreamKind,
    input: Pin<Box<dyn AsyncRead + Send>>,
    output: DuplexStream,
}

/// Consuming an attachment transfers both halves without a placeholder or replacement reader.
pub(super) struct StreamIo {
    pub(super) kind: StreamKind,
    pub(super) reader: FrameReader,
    pub(super) writer: DuplexStream,
}

impl StreamAttachment {
    /// Return the response body while retaining only its bounded writer.
    pub(crate) fn new(kind: StreamKind, body: Body) -> (Self, Body) {
        let input = StreamReader::new(BodyDataStream::new(body).map_err(std::io::Error::other));
        let (output, response) = tokio::io::duplex(af_stream::CHUNK_BYTES);
        let body = Body::from_stream(ReaderStream::with_capacity(
            response,
            af_stream::CHUNK_BYTES,
        ));
        (
            Self {
                kind,
                input: Box::pin(input),
                output,
            },
            body,
        )
    }

    /// The reader and writer now belong to their supervising session.
    pub(super) fn into_io(self, budget: Budget, stop: CancellationToken) -> StreamIo {
        StreamIo {
            kind: self.kind,
            writer: self.output,
            reader: Reader::new(self.input, budget, self.kind, stop, STREAM_ANSWER_TIMEOUT),
        }
    }
}

/// Owning the frame retains its allocation permit through request processing.
pub(super) struct IncomingFrame {
    pub(super) kind: StreamKind,
    pub(super) frame: OwnedFrame,
}

/// Four validated queues also keep activity's empty response open for the whole session.
#[derive(Clone)]
pub(super) struct StreamWriters(BTreeMap<StreamKind, WriteQueue>);

impl TryFrom<BTreeMap<StreamKind, WriteQueue>> for StreamWriters {
    type Error = crate::Error;

    fn try_from(queues: BTreeMap<StreamKind, WriteQueue>) -> Result<Self> {
        if !StreamKind::ALL.iter().all(|kind| queues.contains_key(kind)) {
            return Err(ErrorKind::Protocol.into());
        }
        Ok(Self(queues))
    }
}

impl StreamWriters {
    /// Completion means the frame reached the response pipe; the native listener owns socket drain.
    pub(super) async fn send(
        &self,
        kind: StreamKind,
        frame: OwnedFrame,
        stop: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        let sender = self.0.get(&kind).ok_or(ErrorKind::Protocol)?;
        let (flushed, completion) = oneshot::channel();
        within(stop, deadline, Operation::Write, async {
            sender
                .send(PendingWrite { frame, flushed })
                .await
                .map_err(|_| ErrorKind::Closed)?;
            completion.await.map_err(|_| ErrorKind::Closed)?;
            Ok(())
        })
        .await
    }
}

/// The producer observes a dropped writer through the missing completion acknowledgement.
pub(super) struct PendingWrite {
    frame: OwnedFrame,
    flushed: oneshot::Sender<()>,
}

/// Independent stream readers prevent activity pressure from filling control's queue.
pub(super) async fn read_stream(
    mut reader: FrameReader,
    kind: StreamKind,
    incoming: mpsc::Sender<IncomingFrame>,
    stop: CancellationToken,
) -> Result<()> {
    loop {
        let frame = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            frame = reader.next() => frame?.ok_or(ErrorKind::Closed)?,
        };
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            result = incoming.send(IncomingFrame { kind, frame }) => { result.map_err(|_| ErrorKind::Closed)?; }
        }
    }
}

/// Dropping the returned queue ends this writer after its already-accepted frames.
pub(super) fn writer(
    mut output: DuplexStream,
    stop: CancellationToken,
    workers: &mut JoinSet<Result<()>>,
) -> WriteQueue {
    let (sender, mut receiver) = mpsc::channel::<PendingWrite>(STREAM_QUEUE_DEPTH);
    workers.spawn(async move {
        while let Some(queued) = receiver.recv().await {
            af_stream::write(
                &mut output,
                &queued.frame,
                &stop,
                Instant::now() + STREAM_ANSWER_TIMEOUT,
            )
            .await?;
            let _caller_finished = queued.flushed.send(());
        }
        Ok(())
    });
    sender
}
