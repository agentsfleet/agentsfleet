//! Correlation registrations release on answer, timeout, cancellation and disconnect.

use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use af_stream::OwnedFrame;
use af_wire::protocol::{Down, PENDING_MAX};
use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use crate::{Result, error::ErrorKind};

/// A caller accepts exactly one response variant and, where present, one lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Expected {
    Heartbeat,
    Pong,
    Renewed(Vec<af_core::id::Uuid7>),
    Report(af_core::id::Uuid7),
    Folds(af_core::id::Uuid7),
    AuthoringModel {
        lease: af_core::id::Uuid7,
        dispatch: String,
    },
    AuthoringActivity(af_core::id::Uuid7),
    AuthoringRenew(af_core::id::Uuid7),
    AuthoringReport(af_core::id::Uuid7),
}

struct Pending {
    expected: Expected,
    answer: oneshot::Sender<Result<OwnedFrame>>,
}

pub(crate) struct Requests {
    next: AtomicU64,
    capacity: Arc<Semaphore>,
    entries: Mutex<HashMap<NonZeroU64, Pending>>,
}

impl std::fmt::Debug for Requests {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Requests")
            .field("pending", &self.entries.lock().len())
            .finish_non_exhaustive()
    }
}

impl Requests {
    /// Hello and confirm reserve identifier one for the opening exchange.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            next: AtomicU64::new(2),
            capacity: Arc::new(Semaphore::new(PENDING_MAX)),
            entries: Mutex::new(HashMap::new()),
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
        let mut entries = self.entries.lock();
        if self.capacity.is_closed() {
            return Err(ErrorKind::Disconnected.into());
        }
        entries.insert(id, Pending { expected, answer });
        drop(entries);
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
        let Some(pending) = self.entries.lock().remove(&id) else {
            tracing::debug!(event = "runner_stream_late_answer");
            return Ok(None);
        };
        if !pending.expected.accepts(message) {
            drop(pending.answer.send(Err(ErrorKind::StreamProtocol.into())));
            return Err(ErrorKind::StreamProtocol.into());
        }
        Ok(Some(Reply(pending.answer)))
    }

    pub(crate) fn close(&self) {
        self.capacity.close();
        for (_, pending) in self.entries.lock().drain() {
            drop(pending.answer.send(Err(ErrorKind::Disconnected.into())));
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
            (Self::Heartbeat, Down::Heartbeat { .. }) | (Self::Pong, Down::Pong { .. }) => true,
            (Self::Report(expected), Down::ReportAck { lease_id, .. })
            | (Self::Folds(expected), Down::Folded { lease_id, .. })
            | (Self::AuthoringActivity(expected), Down::AuthoringActivityAck { lease_id, .. })
            | (Self::AuthoringRenew(expected), Down::AuthoringRenewed { lease_id, .. })
            | (Self::AuthoringReport(expected), Down::AuthoringReportAck { lease_id, .. }) => {
                expected.as_str() == lease_id
            }
            (
                Self::AuthoringModel { lease, dispatch },
                Down::AuthoringModelResult {
                    lease_id, answer, ..
                },
            ) => {
                lease.as_str() == lease_id
                    && match answer {
                        af_wire::protocol::Answer::Ok { response } => {
                            &response.dispatch_id == dispatch
                        }
                        af_wire::protocol::Answer::Refused { .. } => true,
                    }
            }
            (Self::Renewed(expected), Down::Renewed { leases, .. }) => {
                let ids: std::collections::HashSet<_> =
                    leases.iter().map(|entry| entry.lease_id.as_ref()).collect();
                ids.len() == leases.len()
                    && ids.len() == expected.len()
                    && expected.iter().all(|lease| ids.contains(lease.as_str()))
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
        self.requests.entries.lock().remove(&self.id);
    }
}
