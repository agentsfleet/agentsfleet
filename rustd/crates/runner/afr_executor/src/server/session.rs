//! One connection's calls: decoded, routed, answered.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use futures_util::StreamExt as _;
use jsonrpsee_types::error::{
    INVALID_REQUEST_CODE, METHOD_NOT_FOUND_CODE, OVERSIZED_REQUEST_CODE, PARSE_ERROR_CODE,
};
use jsonrpsee_types::{Id, Request};
use serde::Serialize;
use tokio::io::AsyncRead;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinSet;
use tokio_util::codec::{AnyDelimiterCodec, AnyDelimiterCodecError, FramedRead};
use tokio_util::sync::{CancellationToken, DropGuard};

use super::files::Workspace;
use super::launch::{InnerEnvironment, Placement, Plan};
use super::process::ProcessRun;
use crate::error::{self, Result};
use crate::protocol::{
    CountParams, Counted, DELIMITER, KillParams, METHOD_COUNT, METHOD_DELETE_FILE, METHOD_HOSTS,
    METHOD_KILL, METHOD_LIST_DIR, METHOD_READ_FILE, METHOD_RECOVERY_IDLE, METHOD_RENEW,
    METHOD_SEARCH, METHOD_SPAWN, METHOD_WRITE, METHOD_WRITE_FILE, PathParams, ReadParams,
    RecoveryParams, SpawnParams, SpawnResult, WriteFileParams, WriteParams,
};
use crate::sizes::Sizes;

/// A line that is not JSON-RPC at all.
const DETAIL_UNREADABLE: &str = "the message is not a JSON-RPC request";
/// A line past the frame cap.
const DETAIL_OVERSIZED: &str = "the message is longer than the executor reads";
/// A method this executor does not serve.
const DETAIL_NO_METHOD: &str = "no such method";
/// A process's input closed; later writes are refused.
const EVENT_INPUT_CLOSED: &str = "executor_input_closed";

/// The session's hold on one running process: where its input goes, and the
/// token that stops it. Dropping the hold cancels the token, so a session that
/// ends stops every process it started.
struct Running {
    input: mpsc::Sender<Bytes>,
    stop: CancellationToken,
    _stops_when_dropped: DropGuard,
}

impl Running {
    fn new(input: mpsc::Sender<Bytes>, stop: CancellationToken) -> Self {
        Self {
            input,
            _stops_when_dropped: stop.clone().drop_guard(),
            stop,
        }
    }
}

/// The state one connection owns.
pub(super) struct Session {
    workspace: Arc<Workspace>,
    /// Where every process the session starts is placed.
    placement: Arc<dyn Placement>,
    /// The environment every process it starts runs in, when it builds one.
    inner: Option<Arc<dyn InnerEnvironment>>,
    outbound: mpsc::UnboundedSender<Bytes>,
    /// Where each process's output and exit go, bounded.
    output: mpsc::Sender<Bytes>,
    processes: HashMap<u64, Running>,
    running: JoinSet<u64>,
    /// File calls on the blocking pool; each answers with a transfer it
    /// finished, for the session to forget.
    calls: JoinSet<Option<u64>>,
    next_process: u64,
    /// Trees crossing in chunks, by the number the client names them by.
    transfers: HashMap<u64, Transfer>,
    next_transfer: u64,
    /// The read clamp and the frame cap its starter chose.
    sizes: Sizes,
}

impl Session {
    /// A session answering through `outbound`, its processes placed by
    /// `placement`, run inside `inner` and speaking through `output`, its
    /// reads and lines held to `sizes`.
    pub(super) fn new(
        workspace: Arc<Workspace>,
        placement: Arc<dyn Placement>,
        inner: Option<Arc<dyn InnerEnvironment>>,
        outbound: mpsc::UnboundedSender<Bytes>,
        output: mpsc::Sender<Bytes>,
        sizes: Sizes,
    ) -> Self {
        Self {
            workspace,
            placement,
            inner,
            outbound,
            output,
            processes: HashMap::new(),
            running: JoinSet::new(),
            calls: JoinSet::new(),
            next_process: 0,
            transfers: HashMap::new(),
            next_transfer: 0,
            sizes,
        }
    }

    /// Serves calls until the connection closes, then ends every process.
    ///
    /// A line past the frame cap is answered, then ends the connection: the
    /// codec stops at the error, and a peer that sent one is not speaking the
    /// protocol.
    pub(super) async fn run(mut self, read: impl AsyncRead + Unpin) {
        let codec = AnyDelimiterCodec::new_with_max_length(
            vec![DELIMITER],
            vec![DELIMITER],
            self.sizes.max_frame_bytes,
        );
        let mut lines = FramedRead::new(read, codec);
        loop {
            tokio::select! {
                frame = lines.next() => match frame {
                    Some(Ok(frame)) => self.dispatch(&frame),
                    Some(Err(AnyDelimiterCodecError::MaxChunkLengthExceeded)) => {
                        self.refuse(Id::Null, OVERSIZED_REQUEST_CODE, DETAIL_OVERSIZED);
                    }
                    Some(Err(AnyDelimiterCodecError::Io(_))) | None => break,
                },
                Some(ended) = self.running.join_next(), if !self.running.is_empty() => {
                    if let Ok(process) = ended {
                        self.processes.remove(&process);
                    }
                }
                Some(served) = self.calls.join_next(), if !self.calls.is_empty() => {
                    self.forget(served.ok().flatten());
                }
            }
        }
        // Dropping every hold is what tells each process to stop.
        self.processes.clear();
        while self.running.join_next().await.is_some() {}
        self.calls.shutdown().await;
    }

    /// Decodes one line and routes it.
    fn dispatch(&mut self, frame: &[u8]) {
        match af_core::json::object_from_slice::<Request<'_>>(frame) {
            Ok(request) => self.route(&request),
            Err(failure) => {
                let code = if failure.is_syntax() || failure.is_eof() {
                    PARSE_ERROR_CODE
                } else {
                    INVALID_REQUEST_CODE
                };
                self.refuse(Id::Null, code, DETAIL_UNREADABLE);
            }
        }
    }

    /// Runs the call a request names.
    fn route(&mut self, request: &Request<'_>) {
        let id = request.id.clone().into_owned();
        match request.method.as_ref() {
            METHOD_RECOVERY_IDLE => {
                let idle = params::<RecoveryParams>(request).and_then(|_params| {
                    while let Some(served) = self.calls.try_join_next() {
                        self.forget(served.ok().flatten());
                    }
                    // A process a call started and left behind is the
                    // tenant's still, though no call of this session runs.
                    Ok(self.calls.is_empty() && self.placement.idle()?)
                });
                self.answer(id, idle);
            }
            METHOD_SPAWN => self.spawn(id, params::<SpawnParams<'static>>(request)),
            METHOD_WRITE => {
                let written = params::<WriteParams>(request)
                    .and_then(|write| self.write(write.process_id, write.data));
                self.answer(id, written);
            }
            METHOD_KILL => {
                let killed =
                    params::<KillParams>(request).and_then(|kill| self.kill(kill.process_id));
                self.answer(id, killed);
            }
            METHOD_READ_FILE => {
                let most = self.sizes.max_read_bytes;
                self.on_files(
                    id,
                    params(request),
                    move |workspace, read: ReadParams<'static>| {
                        workspace.read(&read.path, read.max_bytes.min(most))
                    },
                );
            }
            METHOD_WRITE_FILE => self.on_files(
                id,
                params(request),
                |workspace, write: WriteFileParams<'static>| {
                    workspace.write(&write.path, &write.content)
                },
            ),
            METHOD_DELETE_FILE => self.on_files(
                id,
                params(request),
                |workspace, delete: PathParams<'static>| workspace.delete(&delete.path),
            ),
            METHOD_LIST_DIR => self.on_files(
                id,
                params(request),
                |workspace, list: PathParams<'static>| workspace.list(&list.path),
            ),
            METHOD_SEARCH => self.search(id, params(request)),
            METHOD_RENEW => self.renew(id, params(request)),
            METHOD_HOSTS => self.hosts(id, params(request)),
            METHOD_COUNT => {
                let counted =
                    params::<CountParams>(request).and_then(|params| match params.count {
                        Counted::PidsRefused => Ok(self.placement.pids_refused()?),
                    });
                self.answer(id, counted);
            }
            _other if self.transfer(id.clone(), request) => {}
            _unknown => self.refuse(id, METHOD_NOT_FOUND_CODE, DETAIL_NO_METHOD),
        }
    }

    /// Starts a process: its answer is queued before its first output can be.
    fn spawn(&mut self, id: Id<'static>, params: Result<SpawnParams<'static>>) {
        let started = params
            .and_then(|spawn| Plan::new(spawn, &self.workspace))
            .map(|plan| plan.inside(self.inner.clone()))
            .and_then(|plan| ProcessRun::start(&plan, &self.placement));
        match started {
            Ok((run, input)) => {
                let process = self.next_process;
                self.next_process += 1;
                self.answer(
                    id,
                    Ok(SpawnResult {
                        process_id: process,
                    }),
                );
                let stop = CancellationToken::new();
                self.processes
                    .insert(process, Running::new(input, stop.clone()));
                self.running
                    .spawn(run.drive(process, stop, self.output.clone()));
            }
            Err(failure) => self.answer::<SpawnResult>(id, Err(failure)),
        }
    }

    /// Queues bytes for a process's input without waiting on it: answered
    /// once queued, refused when the queue is full or the input closed.
    fn write(&self, process: u64, data: Bytes) -> Result<()> {
        self.live(process)?
            .input
            .try_send(data)
            .map_err(|refused| match refused {
                TrySendError::Full(_) => error::input_backlog_full(),
                TrySendError::Closed(_) => {
                    let event = EVENT_INPUT_CLOSED;
                    tracing::debug!(event, process_id = process, "a process's input is closed");
                    error::input_closed()
                }
            })
    }

    /// Starts stopping a process; acknowledged as soon as stopping begins.
    fn kill(&self, process: u64) -> Result<()> {
        self.live(process).map(|running| running.stop.cancel())
    }

    /// A process still taking calls: neither stopping nor ended.
    fn live(&self, process: u64) -> Result<&Running> {
        self.processes
            .get(&process)
            .filter(|running| !running.stop.is_cancelled())
            .ok_or_else(error::unknown_process)
    }

    /// Runs a file call on the blocking pool: the workspace handle blocks.
    fn on_files<P, T>(
        &mut self,
        id: Id<'static>,
        params: Result<P>,
        call: impl FnOnce(&Workspace, P) -> Result<T> + Send + 'static,
    ) where
        P: Send + 'static,
        T: Serialize + Clone + Send + 'static,
    {
        match params {
            Ok(params) => self.blocking(id, move |workspace| (call(workspace, params), None)),
            Err(failure) => self.answer::<T>(id, Err(failure)),
        }
    }

    /// Runs `call` on the blocking pool and answers with its result; the
    /// transfer it names, if any, is forgotten once it is done.
    fn blocking<T>(
        &mut self,
        id: Id<'static>,
        call: impl FnOnce(&Workspace) -> (Result<T>, Option<u64>) + Send + 'static,
    ) where
        T: Serialize + Clone + Send + 'static,
    {
        let workspace = Arc::clone(&self.workspace);
        let outbound = self.outbound.clone();
        self.calls.spawn_blocking(move || {
            let (outcome, finished) = call(&workspace);
            post(&outbound, reply(id, outcome));
            finished
        });
    }

    /// Queues a result or a refusal for `id`.
    fn answer<T: Serialize + Clone>(&self, id: Id<'static>, outcome: Result<T>) {
        post(&self.outbound, reply(id, outcome));
    }

    /// Queues a protocol refusal that no call produced.
    fn refuse(&self, id: Id<'static>, code: i32, detail: &'static str) {
        let event = EVENT_CALL_REFUSED;
        tracing::debug!(event, code, detail, "the executor refused a message");
        post(&self.outbound, refusal(id, code, detail));
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod recovery_tests;
#[path = "session_renew.rs"]
mod renew;
#[path = "session_search.rs"]
mod search;
#[path = "session_transfer.rs"]
mod transfer;
#[path = "session_wire.rs"]
mod wire;

use self::transfer::Transfer;
use self::wire::{EVENT_CALL_REFUSED, params, post, refusal, reply};
