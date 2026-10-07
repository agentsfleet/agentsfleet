//! A call on its way to the executor, and the kill a process's events send
//! when their reader leaves before the process ended.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use afd_core::error_code;
use bytes::Bytes;
use serde_json::value::RawValue;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};

use crate::api::{Process, ProcessId};
use crate::error::{Error, Result};
use crate::protocol::{KillParams, METHOD_KILL, request};

/// A kill for a process whose reader left, which could not be queued because
/// the calls waiting for the link filled their backlog.
const EVENT_KILL_UNQUEUED: &str = "executor_kill_unqueued";

/// The number each call carries, so its answer finds its way back. Shared by
/// every caller and by the link, which sends a kill of its own.
#[derive(Debug, Default)]
pub(super) struct CallIds(AtomicU64);

impl CallIds {
    /// The next unused number.
    pub(super) fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

/// One call on its way to the executor, already a line.
pub(super) struct Call {
    /// The number its answer carries.
    pub(super) id: u64,
    /// The request, delimited and ready to write.
    pub(super) line: Bytes,
    /// Where its answer goes.
    pub(super) reply: Reply,
}

/// Where an answer goes, and what it becomes on the way.
pub(super) enum Reply {
    /// The raw result, for the caller to decode.
    Value(oneshot::Sender<Result<Box<RawValue>>>),
    /// A started process, registered for its events before the caller sees it.
    Process(oneshot::Sender<Result<Process>>),
}

impl Reply {
    /// Answers with a failure.
    pub(super) fn fail(self, failure: Error) {
        let _caller_gone = match self {
            Self::Value(sender) => sender.send(Err(failure)).is_ok(),
            Self::Process(sender) => sender.send(Err(failure)).is_ok(),
        };
    }
}

/// The kill that ends `process`, numbered from `ids`; its answer is heard by
/// no one.
pub(super) fn kill(ids: &CallIds, process: ProcessId) -> Option<Call> {
    let id = ids.next();
    let params = KillParams {
        process_id: process.get(),
    };
    let (reply, _unheard) = oneshot::channel();
    // A struct of one integer always encodes.
    request(id, METHOD_KILL, &params).ok().map(|line| Call {
        id,
        line,
        reply: Reply::Value(reply),
    })
}

/// What `process`'s events run when their reader leaves before it ended — a
/// tool call cancelled mid-command — so the command does not run on, unread,
/// to its timeout in a workspace someone else now works in.
///
/// Fire and forget, by design: it runs in a drop, where nothing can wait and
/// no caller hears a failure. `calls` is weak, so a process kept open never
/// keeps its connection open; once the client is gone it sends nothing, since
/// closing the connection ends every process. The executor's answer, a
/// refusal when the process ended meanwhile or was killed already, reaches no
/// one. Only a full backlog loses the kill, and that is logged: the command
/// then runs to its timeout.
pub(super) fn kill_on_abandon(
    calls: mpsc::WeakSender<Call>,
    ids: Arc<CallIds>,
    process: ProcessId,
) -> impl FnOnce() + Send + Sync + 'static {
    move || {
        let Some(calls) = calls.upgrade() else {
            return;
        };
        let queued = kill(&ids, process).map(|call| calls.try_send(call));
        if let Some(Err(TrySendError::Full(_))) = queued {
            unqueued(process);
        }
    }
}

/// Logs a kill for `process` that its full backlog would not take.
fn unqueued(process: ProcessId) {
    let event = EVENT_KILL_UNQUEUED;
    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let process_id = process.get();
    tracing::warn!(
        event,
        error_code,
        process_id,
        "a kill for a process whose reader left could not be queued"
    );
}
