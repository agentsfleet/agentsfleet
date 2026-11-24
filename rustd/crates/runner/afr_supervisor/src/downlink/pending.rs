//! Correlation registrations release on answer, timeout, cancellation and disconnect.

use std::{
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use af_stream::OwnedFrame;
use af_wire::protocol::{Down, PENDING_MAX};
use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use crate::{Result, error::ErrorKind};

/// A caller accepts exactly one response variant and, where present, one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Expected {
    Heartbeat,
    /// The answer to the test-only `Ping`: the runner sends no ping in production.
    #[cfg(test)]
    Pong,
    Renewed(Vec<af_core::id::Uuid7>),
    Report(af_core::id::Uuid7),
    Folds(af_core::id::Uuid7),
    ContextLoad(af_core::id::Uuid7),
    ContextSave(af_core::id::Uuid7),
    TurnCheckpoint {
        run: af_core::id::Uuid7,
        operation: af_wire::run::CheckpointOperationId,
        kind: af_wire::run::TurnCheckpointKind,
    },
}

struct Pending {
    expected: Expected,
    answer: oneshot::Sender<Result<OwnedFrame>>,
}

pub(crate) struct Requests {
    next: AtomicU64,
    capacity: Arc<Semaphore>,
    entries: DashMap<NonZeroU64, Pending>,
    /// Whether the daemon has answered a request on this session.
    answered: AtomicBool,
}

impl std::fmt::Debug for Requests {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Requests")
            .field("pending", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl Requests {
    /// Hello and confirm reserve identifier one for the opening exchange.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            next: AtomicU64::new(2),
            capacity: Arc::new(Semaphore::new(PENDING_MAX)),
            entries: DashMap::new(),
            answered: AtomicBool::new(false),
        })
    }

    pub(crate) async fn register(self: &Arc<Self>, expected: Expected) -> Result<Registration> {
        let permit = Arc::clone(&self.capacity)
            .acquire_owned()
            .await
            .map_err(crate::error::stream_disconnected)?;
        let next = self
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_exhausted_counter| ErrorKind::StreamProtocol)?;
        let id = NonZeroU64::new(next).ok_or(ErrorKind::StreamProtocol)?;
        let (answer, receiver) = oneshot::channel();
        // Insert, then look for a close: a close that came first is seen here and the
        // entry withdrawn, and one that comes later finds the entry in its sweep.
        self.entries.insert(id, Pending { expected, answer });
        if self.capacity.is_closed() {
            self.entries.remove(&id);
            return Err(ErrorKind::Disconnected.into());
        }
        Ok(Registration {
            id,
            requests: Arc::clone(self),
            receiver,
            _permit: permit,
        })
    }

    /// Decode once at dispatch, then transfer the charged frame to the waiting caller.
    pub(crate) fn reply(&self, message: &Down<'_>) -> Result<Option<Reply>> {
        let id = message.request_id().ok_or(ErrorKind::StreamProtocol)?;
        let Some((_, pending)) = self.entries.remove(&id) else {
            tracing::debug!(event = "runner_stream_late_answer");
            return Ok(None);
        };
        if !pending.expected.accepts(message) {
            drop(pending.answer.send(Err(ErrorKind::StreamProtocol.into())));
            return Err(ErrorKind::StreamProtocol.into());
        }
        self.answered.store(true, Ordering::Release);
        Ok(Some(Reply(pending.answer)))
    }

    /// Whether the daemon answered any request on this session: the session proved
    /// usable, so a reconnect after it starts its backoff over.
    pub(crate) fn answered(&self) -> bool {
        self.answered.load(Ordering::Acquire)
    }

    pub(crate) fn close(&self) {
        self.capacity.close();
        let ids: Vec<NonZeroU64> = self.entries.iter().map(|entry| *entry.key()).collect();
        for id in ids {
            if let Some((_, pending)) = self.entries.remove(&id) {
                drop(pending.answer.send(Err(ErrorKind::Disconnected.into())));
            }
        }
    }
}

pub(crate) struct Reply(oneshot::Sender<Result<OwnedFrame>>);

impl Reply {
    /// A cancelled caller has relinquished this answer; dropping it returns memory.
    pub(crate) fn send(self, frame: OwnedFrame) {
        drop(self.0.send(Ok(frame)));
    }
}

impl Expected {
    fn accepts(&self, message: &Down<'_>) -> bool {
        match (self, message) {
            (
                Self::TurnCheckpoint {
                    run,
                    operation,
                    kind,
                },
                Down::TurnCheckpoint {
                    run_id,
                    operation_id,
                    kind: received,
                    answer,
                    ..
                },
            ) => {
                run.as_str() == run_id
                    && operation == operation_id
                    && kind == received
                    && match answer {
                        af_wire::protocol::Answer::Ok { response } => response.kind() == *kind,
                        af_wire::protocol::Answer::Refused { .. } => true,
                    }
            }
            (Self::Heartbeat, Down::Heartbeat { .. }) => true,
            #[cfg(test)]
            (Self::Pong, Down::Pong { .. }) => true,
            (Self::Report(expected), Down::ReportAck { run_id, .. })
            | (Self::Folds(expected), Down::Folded { run_id, .. })
            | (Self::ContextLoad(expected), Down::ContextLoaded { run_id, .. })
            | (Self::ContextSave(expected), Down::ContextSaved { run_id, .. }) => {
                expected.as_str() == run_id
            }
            (Self::Renewed(expected), Down::Renewed { runs, .. }) => {
                let ids: std::collections::HashSet<_> =
                    runs.iter().map(|entry| entry.run_id.as_ref()).collect();
                ids.len() == runs.len()
                    && ids.len() == expected.len()
                    && expected.iter().all(|run| ids.contains(run.as_str()))
            }
            _ => false,
        }
    }
}

pub(crate) struct Registration {
    pub(crate) id: NonZeroU64,
    requests: Arc<Requests>,
    receiver: oneshot::Receiver<Result<OwnedFrame>>,
    _permit: OwnedSemaphorePermit,
}

impl Registration {
    pub(crate) async fn receive(&mut self) -> Result<OwnedFrame> {
        (&mut self.receiver)
            .await
            .map_err(crate::error::stream_disconnected)?
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.requests.entries.remove(&self.id);
    }
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;
