//! One connection's calls: decoded, routed, answered.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use futures_util::StreamExt as _;
use jsonrpsee_types::error::{
    INVALID_REQUEST_CODE, METHOD_NOT_FOUND_CODE, OVERSIZED_REQUEST_CODE, PARSE_ERROR_CODE,
};
use jsonrpsee_types::{ErrorObject, Id, Request, Response, ResponsePayload};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::unix::OwnedReadHalf;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinSet;
use tokio_util::codec::{AnyDelimiterCodec, AnyDelimiterCodecError, FramedRead};
use tokio_util::sync::{CancellationToken, DropGuard};

use super::files::Workspace;
use super::launch::Placement;
use super::launch::Plan;
use super::process::ProcessRun;
use crate::error::{self, Result};
use crate::protocol::{
    DELIMITER, KillParams, MAX_FRAME_BYTES, METHOD_APPEND_FILE, METHOD_DELETE_FILE, METHOD_KILL,
    METHOD_LIST_DIR, METHOD_READ_FILE, METHOD_SPAWN, METHOD_WRITE, METHOD_WRITE_FILE, PathParams,
    ReadParams, SpawnParams, SpawnResult, WriteFileParams, WriteParams, decoded, line,
};

/// A line that is not JSON-RPC at all.
const DETAIL_UNREADABLE: &str = "the message is not a JSON-RPC request";
/// A line past the frame cap.
const DETAIL_OVERSIZED: &str = "the message is longer than the executor reads";
/// A method this executor does not serve.
const DETAIL_NO_METHOD: &str = "no such method";
/// A call that needs parameters and sent none.
const DETAIL_NO_PARAMS: &str = "the method takes parameters";
/// The executor refused a call; the code says why.
const EVENT_CALL_REFUSED: &str = "executor_call_refused";
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
    outbound: mpsc::UnboundedSender<Bytes>,
    /// Where each process's output and exit go, bounded.
    output: mpsc::Sender<Bytes>,
    processes: HashMap<u64, Running>,
    running: JoinSet<u64>,
    calls: JoinSet<()>,
    next_process: u64,
}

impl Session {
    /// A session answering through `outbound`, its processes placed by
    /// `placement` and speaking through `output`.
    pub(super) fn new(
        workspace: Arc<Workspace>,
        placement: Arc<dyn Placement>,
        outbound: mpsc::UnboundedSender<Bytes>,
        output: mpsc::Sender<Bytes>,
    ) -> Self {
        Self {
            workspace,
            placement,
            outbound,
            output,
            processes: HashMap::new(),
            running: JoinSet::new(),
            calls: JoinSet::new(),
            next_process: 0,
        }
    }

    /// Serves calls until the connection closes, then ends every process.
    ///
    /// A line past the frame cap is answered, then ends the connection: the
    /// codec stops at the error, and a peer that sent one is not speaking the
    /// protocol.
    pub(super) async fn run(mut self, read: OwnedReadHalf) {
        let codec = AnyDelimiterCodec::new_with_max_length(
            vec![DELIMITER],
            vec![DELIMITER],
            MAX_FRAME_BYTES,
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
                Some(_served) = self.calls.join_next(), if !self.calls.is_empty() => {}
            }
        }
        // Dropping every hold is what tells each process to stop.
        self.processes.clear();
        while self.running.join_next().await.is_some() {}
        self.calls.shutdown().await;
    }

    /// Decodes one line and routes it.
    fn dispatch(&mut self, frame: &[u8]) {
        match afd_core::json::object_from_slice::<Request<'_>>(frame) {
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
            METHOD_READ_FILE => self.on_files(
                id,
                params(request),
                |workspace, read: ReadParams<'static>| workspace.read(&read.path, read.max_bytes),
            ),
            METHOD_WRITE_FILE => self.on_files(
                id,
                params(request),
                |workspace, write: WriteFileParams<'static>| {
                    workspace.write(&write.path, &write.content)
                },
            ),
            METHOD_APPEND_FILE => self.on_files(
                id,
                params(request),
                |workspace, append: WriteFileParams<'static>| {
                    workspace.append(&append.path, &append.content)
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
            _unknown => self.refuse(id, METHOD_NOT_FOUND_CODE, DETAIL_NO_METHOD),
        }
    }

    /// Starts a process: its answer is queued before its first output can be.
    fn spawn(&mut self, id: Id<'static>, params: Result<SpawnParams<'static>>) {
        let started = params
            .and_then(|spawn| Plan::new(spawn, &self.workspace))
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
            Ok(params) => {
                let workspace = Arc::clone(&self.workspace);
                let outbound = self.outbound.clone();
                self.calls
                    .spawn_blocking(move || post(&outbound, reply(id, call(&workspace, params))));
            }
            Err(failure) => self.answer::<T>(id, Err(failure)),
        }
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

/// A request's parameters, decoded through the object-only gate: whatever is
/// on the other end of the socket is not trusted.
fn params<T: DeserializeOwned>(request: &Request<'_>) -> Result<T> {
    let raw = request
        .params
        .as_deref()
        .ok_or_else(|| error::invalid_params(DETAIL_NO_PARAMS))?;
    Ok(decoded(raw)?)
}

/// The line answering `id` with `outcome`.
///
/// A refusal is logged by its code alone: its sentence can quote what the
/// call carried — an environment value, a path — and the sandbox's error
/// stream reaches the host's journal.
fn reply<T: Serialize + Clone>(id: Id<'static>, outcome: Result<T>) -> Bytes {
    match outcome {
        Ok(value) => line(&Response::new(ResponsePayload::success(value), id)),
        Err(failure) => {
            let event = EVENT_CALL_REFUSED;
            let code = failure.rpc_code();
            tracing::debug!(event, code, "the executor refused a call");
            refusal(id, code, failure.wire_message())
        }
    }
}

/// The line refusing `id` with `code` and `message`.
fn refusal(id: Id<'static>, code: i32, message: impl Into<String>) -> Bytes {
    let refusal = ErrorObject::owned(code, message, None::<()>);
    line(&Response::<()>::new(ResponsePayload::error(refusal), id))
}

/// Queues a line for the writer.
///
/// A send fails only once the writer has ended, which means the supervisor
/// is gone and there is no one left to tell.
fn post(outbound: &mpsc::UnboundedSender<Bytes>, line: Bytes) {
    let _writer_gone = outbound.send(line);
}
