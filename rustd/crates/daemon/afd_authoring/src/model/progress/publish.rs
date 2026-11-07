//! Current publication authority is checked without holding a connection over a bus write.

use super::trace::Trace;
use super::{CAPACITY, Delta, Piece, Preview, TextKind, draft_channel};
use crate::{Drafts, error::raise::query, lease::guard};
use afd_core::id::Uuid7;
use afd_dragonfly::FleetStreams;
use sqlx::Connection as _;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::task::AbortOnDropHandle;
use zeroize::Zeroizing;

pub(super) const DEADLINE: Duration = Duration::from_secs(2);
const CONTEXT: &str = "authorize private answer preview";

pub(in crate::model) struct Publisher(AbortOnDropHandle<()>);

impl Publisher {
    pub(in crate::model) fn start(
        streams: FleetStreams,
        drafts: Drafts,
        runner: Uuid7,
        lease: Uuid7,
        counters: guard::Counters,
        dispatch: Uuid7,
    ) -> (Self, Preview) {
        let (sender, receiver) = mpsc::channel::<Piece>(CAPACITY);
        // Construct before spawn so cancellation before the first poll is paired.
        let trace = Trace::start(dispatch.clone());
        let worker = Worker {
            streams,
            drafts,
            runner,
            lease,
            counters,
            dispatch,
            trace,
        };
        let task = tokio::spawn(worker.run(receiver));
        (Self(AbortOnDropHandle::new(task)), Preview::new(sender))
    }

    pub(in crate::model) async fn drain(self) {
        // The handle aborts on timeout, HTTP cancellation or unwind.
        let _finished = tokio::time::timeout(DEADLINE, self.0).await;
    }
}

struct Worker {
    streams: FleetStreams,
    drafts: Drafts,
    runner: Uuid7,
    lease: Uuid7,
    counters: guard::Counters,
    dispatch: Uuid7,
    trace: Trace,
}

impl Worker {
    async fn run(mut self, mut receiver: mpsc::Receiver<Piece>) {
        while let Some(piece) = receiver.recv().await {
            match tokio::time::timeout(DEADLINE, self.publish(&piece)).await {
                Ok(Ok(())) => self.trace.published(),
                Ok(Err(error)) => {
                    self.trace.failed(error.code(), "publication_refused");
                    return;
                }
                Err(_) => {
                    self.trace
                        .failed(afd_core::error_code::INTERNAL_OPERATION_FAILED, "deadline");
                    return;
                }
            }
        }
        self.trace.completed();
    }

    async fn publish(&self, piece: &Piece) -> crate::Result<()> {
        let (draft_id, sequence) =
            authorize(&self.drafts, &self.runner, &self.lease, &self.counters).await?;
        let frame = Delta {
            draft_id,
            sequence: sequence.to_string(),
            stream_id: self.dispatch.clone(),
            stream_seq: piece.sequence,
            stream_start: piece.sequence == 1,
            text_kind: TextKind::Answer,
            text: piece.text.to_string(),
        };
        let payload =
            Zeroizing::new(serde_json::to_string(&frame).map_err(crate::error::raise::stored)?);
        self.streams
            .publish(&draft_channel(&frame.draft_id), &payload)
            .await
            .map(|_subscribers| ())
            .map_err(|source| crate::error::ErrorKind::Preview { source }.into())
    }
}

pub(super) async fn authorize(
    drafts: &Drafts,
    runner: &Uuid7,
    lease: &Uuid7,
    counters: &guard::Counters,
) -> crate::Result<(Uuid7, i64)> {
    let mut connection = drafts.database.acquire().await?;
    let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
    let guarded = guard::publication(&mut tx, runner, lease, counters).await?;
    let identity = (guarded.attempt.draft_id, guarded.attempt.message_sequence);
    tx.commit().await.map_err(query(CONTEXT))?;
    drop(connection);
    Ok(identity)
}
