//! One connection's calls: decoded, routed, answered.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::StreamExt as _;
use jsonrpsee_types::error::{
    INVALID_REQUEST_CODE, METHOD_NOT_FOUND_CODE, OVERSIZED_REQUEST_CODE, PARSE_ERROR_CODE,
};
use jsonrpsee_types::{ErrorObject, Id, Request, Response, ResponsePayload};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::unix::OwnedReadHalf;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

use super::files::Workspace;
use super::launch::Plan;
use super::process::{Control, ProcessRun, Steer};
use crate::error::{self, Error, Result};
use crate::protocol::{
    KillParams, ListParams, MAX_FRAME_BYTES, METHOD_KILL, METHOD_LIST_DIR, METHOD_READ_FILE,
    METHOD_SPAWN, METHOD_WRITE, METHOD_WRITE_FILE, ReadParams, SpawnParams, SpawnResult,
    WriteFileParams, WriteParams, decode, line,
};

/// How many control messages may wait for one process.
const CONTROL_BACKLOG: usize = 16;
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

/// The state one connection owns.
pub(super) struct Session {
    workspace: Arc<Workspace>,
    outbound: mpsc::UnboundedSender<String>,
    controls: HashMap<u64, mpsc::Sender<Control>>,
    running: JoinSet<u64>,
    calls: JoinSet<()>,
    next_process: u64,
}

impl Session {
    /// A session answering through `outbound`.
    pub(super) fn new(workspace: Arc<Workspace>, outbound: mpsc::UnboundedSender<String>) -> Self {
        Self {
            workspace,
            outbound,
            controls: HashMap::new(),
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
        let mut lines = FramedRead::new(read, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));
        loop {
            tokio::select! {
                frame = lines.next() => match frame {
                    Some(Ok(text)) => self.dispatch(&text),
                    Some(Err(LinesCodecError::MaxLineLengthExceeded)) => {
                        self.refuse(Id::Null, OVERSIZED_REQUEST_CODE, DETAIL_OVERSIZED);
                    }
                    Some(Err(LinesCodecError::Io(_))) | None => break,
                },
                Some(ended) = self.running.join_next(), if !self.running.is_empty() => {
                    if let Ok(process) = ended {
                        self.controls.remove(&process);
                    }
                }
                Some(_served) = self.calls.join_next(), if !self.calls.is_empty() => {}
            }
        }
        // Dropping every control channel is what tells each process to stop.
        self.controls.clear();
        while self.running.join_next().await.is_some() {}
        self.calls.shutdown().await;
    }

    /// Decodes one line and routes it.
    fn dispatch(&mut self, text: &str) {
        match afd_core::json::object_from_slice::<Request<'_>>(text.as_bytes()) {
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
            METHOD_WRITE => self.steer(
                id,
                params::<WriteParams>(request)
                    .and_then(|write| Ok((write.process_id, Steer::Write(decode(&write.data)?)))),
            ),
            METHOD_KILL => self.steer(
                id,
                params::<KillParams>(request).map(|kill| (kill.process_id, Steer::Kill)),
            ),
            METHOD_READ_FILE => self.on_files(
                id,
                params(request),
                |workspace, read: ReadParams<'static>| workspace.read(&read.path, read.max_bytes),
            ),
            METHOD_WRITE_FILE => self.on_files(
                id,
                params(request),
                |workspace, write: WriteFileParams<'static>| {
                    workspace.write(&write.path, &decode(&write.content)?)
                },
            ),
            METHOD_LIST_DIR => self.on_files(
                id,
                params(request),
                |workspace, list: ListParams<'static>| workspace.list(&list.path),
            ),
            _unknown => self.refuse(id, METHOD_NOT_FOUND_CODE, DETAIL_NO_METHOD),
        }
    }

    /// Starts a process: its answer is queued before its first output can be.
    fn spawn(&mut self, id: Id<'static>, params: Result<SpawnParams<'static>>) {
        let started = params
            .and_then(|spawn| Plan::new(spawn, &self.workspace))
            .and_then(|plan| ProcessRun::start(&plan));
        match started {
            Ok(run) => {
                let process = self.next_process;
                self.next_process += 1;
                self.answer(
                    id,
                    Ok(SpawnResult {
                        process_id: process,
                    }),
                );
                let (control, controls) = mpsc::channel(CONTROL_BACKLOG);
                self.controls.insert(process, control);
                self.running
                    .spawn(run.drive(process, controls, self.outbound.clone()));
            }
            Err(failure) => self.answer::<SpawnResult>(id, Err(failure)),
        }
    }

    /// Hands a write or a kill to the process it names.
    fn steer(&mut self, id: Id<'static>, target: Result<(u64, Steer)>) {
        let routed = target.and_then(|(process, steer)| {
            self.controls
                .get(&process)
                .cloned()
                .map(|control| (control, steer))
                .ok_or_else(error::unknown_process)
        });
        match routed {
            Ok((control, steer)) => {
                let outbound = self.outbound.clone();
                self.calls.spawn(async move {
                    post(&outbound, reply(id, steer.deliver(&control).await));
                });
            }
            Err(failure) => self.answer::<()>(id, Err(failure)),
        }
    }

    /// Runs a file call off the session task: the workspace handle blocks.
    fn on_files<P, T>(
        &mut self,
        id: Id<'static>,
        params: Result<P>,
        call: impl FnOnce(&Workspace, P) -> Result<T> + Send + 'static,
    ) where
        P: Send + 'static,
        T: Serialize + Clone + Send + 'static,
    {
        let workspace = Arc::clone(&self.workspace);
        let outbound = self.outbound.clone();
        self.calls.spawn(async move {
            let answered = match params {
                Ok(params) => tokio::task::spawn_blocking(move || call(&workspace, params))
                    .await
                    .map_err(Error::from)
                    .and_then(|outcome| outcome),
                Err(failure) => Err(failure),
            };
            post(&outbound, reply(id, answered));
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
        let refusal = ErrorObject::owned(code, detail, None::<()>);
        post(
            &self.outbound,
            line(&Response::<()>::new(ResponsePayload::error(refusal), id)),
        );
    }
}

/// A request's parameters, decoded through the object-only gate: whatever is
/// on the other end of the socket is not trusted.
fn params<T: DeserializeOwned>(request: &Request<'_>) -> Result<T> {
    let raw = request
        .params
        .as_deref()
        .ok_or_else(|| error::invalid_params(DETAIL_NO_PARAMS))?;
    Ok(afd_core::json::object_from_slice(raw.get().as_bytes())?)
}

/// The line answering `id` with `outcome`.
fn reply<T: Serialize + Clone>(id: Id<'static>, outcome: Result<T>) -> String {
    match outcome {
        Ok(value) => line(&Response::new(ResponsePayload::success(value), id)),
        Err(failure) => {
            let event = EVENT_CALL_REFUSED;
            let code = failure.rpc_code();
            let reason = failure.wire_message();
            tracing::debug!(event, code, reason, "the executor refused a call");
            let refusal = ErrorObject::owned(code, reason, None::<()>);
            line(&Response::<()>::new(ResponsePayload::error(refusal), id))
        }
    }
}

/// Queues a line for the writer.
///
/// A send fails only once the writer has ended, which means the supervisor
/// is gone and there is no one left to tell.
pub(super) fn post(outbound: &mpsc::UnboundedSender<String>, text: String) {
    let _writer_gone = outbound.send(text);
}
