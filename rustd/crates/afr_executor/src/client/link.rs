//! The task that owns one executor socket.

use std::collections::HashMap;

use futures_util::{SinkExt as _, StreamExt as _};
use jsonrpsee_types::{Id, Notification, Request, Response, ResponsePayload};
use serde_json::value::RawValue;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio_util::sync::CancellationToken;

use crate::api::{Ending, Process, ProcessEvent, ProcessId};
use crate::error::{self, Error, Result};
use crate::protocol::{
    ExitedParams, KillParams, MAX_FRAME_BYTES, METHOD_KILL, NOTIFY_EXITED, NOTIFY_OUTPUT,
    OutputParams, SpawnResult, decode, line,
};

/// The link took its socket.
const EVENT_LINK_STARTED: &str = "executor_link_started";
/// The connection closed with nothing open.
const EVENT_LINK_COMPLETED: &str = "executor_link_completed";
/// The connection closed, or was given up, with calls or processes open.
const EVENT_LINK_FAILED: &str = "executor_link_failed";
/// A message from the executor did not decode.
const EVENT_MESSAGE_UNREADABLE: &str = "executor_message_unreadable";
/// A call too long for the executor to read, refused before it is sent: the
/// executor would answer it by closing the connection.
const DETAIL_TOO_LONG: &str = "the call is longer than the executor reads";

/// One call on its way to the executor.
pub(super) struct Call {
    /// Its method.
    pub(super) method: &'static str,
    /// Its parameters, already serialized.
    pub(super) params: Box<RawValue>,
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
    fn fail(self, failure: Error) {
        let _caller_gone = match self {
            Self::Value(sender) => sender.send(Err(failure)).is_ok(),
            Self::Process(sender) => sender.send(Err(failure)).is_ok(),
        };
    }
}

/// The socket and everything waiting on it.
pub(super) struct Link {
    lines: FramedRead<OwnedReadHalf, LinesCodec>,
    sink: FramedWrite<OwnedWriteHalf, LinesCodec>,
    calls: mpsc::Receiver<Call>,
    lost: CancellationToken,
    pending: HashMap<u64, Reply>,
    processes: HashMap<ProcessId, mpsc::UnboundedSender<ProcessEvent>>,
    next_call: u64,
}

impl Link {
    /// A link over `stream`, taking calls from `calls`, given up when `lost`
    /// is cancelled.
    pub(super) fn new(
        stream: UnixStream,
        calls: mpsc::Receiver<Call>,
        lost: CancellationToken,
    ) -> Self {
        let (read, write) = stream.into_split();
        Self {
            lines: FramedRead::new(read, LinesCodec::new_with_max_length(MAX_FRAME_BYTES)),
            sink: FramedWrite::new(write, LinesCodec::new()),
            calls,
            lost,
            pending: HashMap::new(),
            processes: HashMap::new(),
            next_call: 0,
        }
    }

    /// Sends calls and routes answers until the socket or the client closes,
    /// or a call's deadline gives the link up.
    pub(super) async fn run(mut self) {
        let event = EVENT_LINK_STARTED;
        tracing::debug!(event, "the executor connection opened");
        let lost = self.lost.clone();
        loop {
            let open = tokio::select! {
                call = self.calls.recv() => match call {
                    Some(call) => self.send_unless_lost(call, &lost).await,
                    None => false,
                },
                frame = self.lines.next() => match frame {
                    Some(Ok(text)) => match self.receive(&text) {
                        Some(orphan) => self.send_unless_lost(abandon(orphan), &lost).await,
                        None => true,
                    },
                    Some(Err(_)) | None => false,
                },
                () = lost.cancelled() => false,
            };
            if !open {
                break;
            }
        }
        self.interrupt();
    }

    /// Sends one call, unless the link is given up first: a stopped executor
    /// stops reading, and a send into a full socket would wait forever.
    async fn send_unless_lost(&mut self, call: Call, lost: &CancellationToken) -> bool {
        tokio::select! {
            sent = self.send(call) => sent.is_ok(),
            () = lost.cancelled() => false,
        }
    }

    /// Writes one call, remembering where its answer goes.
    async fn send(&mut self, call: Call) -> Result<()> {
        let id = self.next_call;
        self.next_call += 1;
        let text = line(&Request::owned(
            call.method.to_owned(),
            Some(call.params),
            Id::Number(id),
        ));
        if text.len() > MAX_FRAME_BYTES {
            call.reply.fail(error::invalid_params(DETAIL_TOO_LONG));
            return Ok(());
        }
        self.pending.insert(id, call.reply);
        Ok(self.sink.send(text).await?)
    }

    /// Routes one line: a notification or an answer. Answers with a process
    /// whose caller left before it started, which the executor must end.
    fn receive(&mut self, text: &str) -> Option<ProcessId> {
        let bytes = text.as_bytes();
        let routed =
            match afd_core::json::object_from_slice::<Notification<'_, Box<RawValue>>>(bytes) {
                Ok(notification) => self
                    .notified(&notification.method, &notification.params)
                    .map(|()| None),
                Err(_not_a_notification) => {
                    afd_core::json::object_from_slice::<Response<'_, Box<RawValue>>>(bytes)
                        .map(|response| self.answered(response))
                        .map_err(Error::from)
                }
            };
        routed.unwrap_or_else(|failure| {
            let event = EVENT_MESSAGE_UNREADABLE;
            let error_code = failure.code().as_str();
            let reason = failure.wire_message();
            tracing::warn!(
                event,
                error_code,
                reason,
                "a message from the executor did not decode"
            );
            None
        })
    }

    /// Hands an answer to the call that waits for it; a started process no
    /// one is waiting for comes back to be ended.
    fn answered(&mut self, response: Response<'_, Box<RawValue>>) -> Option<ProcessId> {
        let outcome = match response.payload {
            ResponsePayload::Success(value) => Ok(value.into_owned()),
            ResponsePayload::Error(refusal) => {
                Err(error::refused(refusal.code(), refusal.message()))
            }
        };
        let waiting = response
            .id
            .as_number()
            .and_then(|id| self.pending.remove(id));
        match waiting {
            Some(Reply::Value(sender)) => {
                let _caller_gone = sender.send(outcome);
                None
            }
            Some(Reply::Process(sender)) => {
                let registered = outcome.and_then(|raw| self.register(&raw));
                sender
                    .send(registered)
                    .err()
                    .and_then(Result::ok)
                    .map(|unclaimed| {
                        self.processes.remove(&unclaimed.id);
                        unclaimed.id
                    })
            }
            None => None,
        }
    }

    /// Opens the event channel of a process the executor just started.
    fn register(&mut self, raw: &RawValue) -> Result<Process> {
        let started: SpawnResult = afd_core::json::object_from_slice(raw.get().as_bytes())?;
        let id = ProcessId::new(started.process_id);
        let (sender, events) = mpsc::unbounded_channel();
        self.processes.insert(id, sender);
        Ok(Process { id, events })
    }

    /// Delivers a process's output or its end.
    fn notified(&mut self, method: &str, params: &RawValue) -> Result<()> {
        let bytes = params.get().as_bytes();
        match method {
            NOTIFY_OUTPUT => {
                let output: OutputParams = afd_core::json::object_from_slice(bytes)?;
                let event = ProcessEvent::Output {
                    stream: output.stream,
                    data: decode(&output.data)?,
                };
                if let Some(sender) = self.processes.get(&ProcessId::new(output.process_id)) {
                    let _caller_gone = sender.send(event);
                }
            }
            NOTIFY_EXITED => {
                let exited: ExitedParams = afd_core::json::object_from_slice(bytes)?;
                let event = ProcessEvent::Ended {
                    ending: exited.ending,
                    omitted_bytes: exited.omitted_bytes,
                };
                if let Some(sender) = self.processes.remove(&ProcessId::new(exited.process_id)) {
                    let _caller_gone = sender.send(event);
                }
            }
            _unknown => {}
        }
        Ok(())
    }

    /// Fails every waiting call and ends every open process, once each.
    fn interrupt(self) {
        let calls = self.pending.len();
        let processes = self.processes.len();
        self.pending
            .into_values()
            .for_each(|reply| reply.fail(error::connection_lost()));
        for sender in self.processes.into_values() {
            let _caller_gone = sender.send(ProcessEvent::Ended {
                ending: Ending::Interrupted,
                omitted_bytes: 0,
            });
        }
        if calls + processes == 0 {
            let event = EVENT_LINK_COMPLETED;
            tracing::debug!(event, "the executor connection closed");
        } else {
            let event = EVENT_LINK_FAILED;
            let error_code = error::connection_lost().code().as_str();
            tracing::warn!(
                event,
                error_code,
                calls,
                processes,
                "the executor connection closed under open work"
            );
        }
    }
}

/// The kill that ends a process no caller is waiting for; its answer is
/// heard by no one.
fn abandon(process: ProcessId) -> Call {
    let params = KillParams {
        process_id: process.get(),
    };
    let (reply, _unheard) = oneshot::channel();
    Call {
        method: METHOD_KILL,
        // A struct of one integer always serializes.
        params: serde_json::value::to_raw_value(&params).unwrap_or_default(),
        reply: Reply::Value(reply),
    }
}
