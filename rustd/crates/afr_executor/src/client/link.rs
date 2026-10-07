//! The task that owns one executor socket.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use afd_core::error_code;
use futures_util::StreamExt as _;
use jsonrpsee_types::{ErrorObject, Id};
use serde::Deserialize;
use serde::de::Error as _;
use serde_json::value::RawValue;
use tokio::io::AsyncWriteExt as _;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio_util::codec::{AnyDelimiterCodec, FramedRead};
use tokio_util::sync::CancellationToken;

use super::call::{self, Call, CallIds, Reply};
use crate::api::{Ending, Process, ProcessId};
use crate::error::{self, Result};
use crate::events::{Events, Feed};
use crate::protocol::{
    DELIMITER, ExitedParams, MAX_FRAME_BYTES, NOTIFY_EXITED, NOTIFY_OUTPUT, OutputParams,
    READ_CHUNK_BYTES, SpawnResult, decoded,
};

/// The link took its socket.
const EVENT_LINK_STARTED: &str = "executor_link_started";
/// The connection closed with nothing open.
const EVENT_LINK_COMPLETED: &str = "executor_link_completed";
/// The connection closed, or was given up, with calls or processes open.
const EVENT_LINK_FAILED: &str = "executor_link_failed";
/// A message from the executor did not decode.
const EVENT_MESSAGE_UNREADABLE: &str = "executor_message_unreadable";
/// A chunk of output longer than one read, which no executor sends: dropped,
/// since a view of it would pin the whole allocation.
const EVENT_OUTPUT_OVERSIZED: &str = "executor_output_oversized";
/// What a message that is neither a notification nor an answer lacks.
const FIELD_METHOD_OR_ID: &str = "method or id";

/// Either message the executor sends, read in one pass: a notification names
/// a method, an answer names the call it answers and carries a result or an
/// error. Codex reads its peer the same way
/// (`codex-rs/exec-server-protocol/src/rpc.rs`), where trying one envelope
/// and then the other would scan every answer twice.
#[derive(Deserialize)]
struct Incoming<'a> {
    #[serde(borrow, default)]
    method: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    params: Option<&'a RawValue>,
    #[serde(borrow, default)]
    id: Option<Id<'a>>,
    #[serde(borrow, default)]
    result: Option<&'a RawValue>,
    #[serde(borrow, default)]
    error: Option<ErrorObject<'a>>,
}

/// The socket and everything waiting on it.
pub(super) struct Link {
    lines: FramedRead<OwnedReadHalf, AnyDelimiterCodec>,
    write: OwnedWriteHalf,
    calls: mpsc::Receiver<Call>,
    /// Where a process's events queue their kill when their reader leaves;
    /// weak, so a process kept open never keeps the connection open.
    outbox: mpsc::WeakSender<Call>,
    ids: Arc<CallIds>,
    lost: CancellationToken,
    pending: HashMap<u64, Reply>,
    processes: HashMap<ProcessId, Feed>,
    /// Whether a chunk past one read was warned about: the first is worth a
    /// warning, the rest a debug line, so a hostile sandbox cannot flood the
    /// journal.
    oversized_warned: bool,
}

impl Link {
    /// A link over `stream`, taking calls from `calls`, whose weak sender
    /// `outbox` each process's events queue their kill on, given up when
    /// `lost` is cancelled.
    pub(super) fn new(
        stream: UnixStream,
        calls: mpsc::Receiver<Call>,
        outbox: mpsc::WeakSender<Call>,
        ids: Arc<CallIds>,
        lost: CancellationToken,
    ) -> Self {
        let (read, write) = stream.into_split();
        let codec = AnyDelimiterCodec::new_with_max_length(
            vec![DELIMITER],
            vec![DELIMITER],
            MAX_FRAME_BYTES,
        );
        Self {
            lines: FramedRead::new(read, codec),
            write,
            calls,
            outbox,
            ids,
            lost,
            pending: HashMap::new(),
            processes: HashMap::new(),
            oversized_warned: false,
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
                    Some(Ok(frame)) => match self.receive(&frame).and_then(|orphan| self.abandon(orphan)) {
                        Some(kill) => self.send_unless_lost(kill, &lost).await,
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
        self.pending.insert(call.id, call.reply);
        tokio::select! {
            sent = self.write.write_all(&call.line) => sent.is_ok(),
            () = lost.cancelled() => false,
        }
    }

    /// Routes one message: a notification or an answer. Answers with a
    /// process whose caller left before it started, which the executor must
    /// end.
    fn receive(&mut self, frame: &[u8]) -> Option<ProcessId> {
        let routed =
            afd_core::json::object_from_slice::<Incoming<'_>>(frame).and_then(|incoming| {
                match (incoming.method, incoming.id) {
                    (Some(method), _) => self.notified(&method, incoming.params).map(|()| None),
                    (None, Some(id)) => {
                        let outcome = match incoming.error {
                            Some(refusal) => Err(error::refused(refusal.code(), refusal.message())),
                            None => Ok(incoming.result.unwrap_or(RawValue::NULL).to_owned()),
                        };
                        Ok(self.answered(&id, outcome))
                    }
                    (None, None) => Err(serde_json::Error::missing_field(FIELD_METHOD_OR_ID)),
                }
            });
        routed.unwrap_or_else(|failure| {
            unreadable(&failure);
            None
        })
    }

    /// Hands an answer to the call that waits for it; a started process no
    /// one is waiting for comes back to be ended.
    fn answered(&mut self, id: &Id<'_>, outcome: Result<Box<RawValue>>) -> Option<ProcessId> {
        match id.as_number().and_then(|id| self.pending.remove(id)) {
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

    /// Opens the events of a process the executor just started, which kill
    /// it when they go before it ended.
    fn register(&mut self, raw: &RawValue) -> Result<Process> {
        let started: SpawnResult = decoded(raw)?;
        let id = ProcessId::new(started.process_id);
        let (feed, mut events) = Events::channel();
        let ids = Arc::clone(&self.ids);
        events.on_abandon(call::kill_on_abandon(self.outbox.clone(), ids, id));
        self.processes.insert(id, feed);
        Ok(Process { id, events })
    }

    /// Delivers a process's output or its end, never waiting on its reader.
    fn notified(&mut self, method: &str, params: Option<&RawValue>) -> serde_json::Result<()> {
        let params = params.unwrap_or(RawValue::NULL);
        match method {
            NOTIFY_OUTPUT => {
                let output: OutputParams = decoded(params)?;
                if output.data.len() > READ_CHUNK_BYTES {
                    self.oversized(output.process_id, output.data.len());
                } else if let Some(feed) = self.processes.get(&ProcessId::new(output.process_id)) {
                    feed.output(output.stream, output.data);
                }
            }
            NOTIFY_EXITED => {
                let exited: ExitedParams = decoded(params)?;
                if let Some(feed) = self.processes.remove(&ProcessId::new(exited.process_id)) {
                    feed.end(exited.ending, exited.output_abandoned);
                }
            }
            _unknown => {}
        }
        Ok(())
    }

    /// Logs a chunk of output longer than one read, dropped unread: the first
    /// at warn, the rest at debug.
    fn oversized(&mut self, process: u64, bytes: usize) {
        let event = EVENT_OUTPUT_OVERSIZED;
        if std::mem::replace(&mut self.oversized_warned, true) {
            tracing::debug!(
                event,
                process_id = process,
                bytes,
                "another oversized chunk"
            );
        } else {
            let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(
                event,
                error_code,
                process_id = process,
                bytes,
                "a chunk of output is longer than one read"
            );
        }
    }

    /// The kill that ends a process no caller is waiting for; its answer is
    /// heard by no one.
    fn abandon(&self, process: ProcessId) -> Option<Call> {
        call::kill(&self.ids, process)
    }

    /// Fails every waiting call and ends every open process, once each.
    fn interrupt(self) {
        let calls = self.pending.len();
        let processes = self.processes.len();
        self.pending
            .into_values()
            .for_each(|reply| reply.fail(error::connection_lost()));
        for feed in self.processes.into_values() {
            feed.end(Ending::Interrupted, false);
        }
        if calls + processes == 0 {
            let event = EVENT_LINK_COMPLETED;
            tracing::debug!(event, "the executor connection closed");
        } else {
            let event = EVENT_LINK_FAILED;
            let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
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

/// Logs a message that did not decode: where it failed, never the decoder's
/// sentence, which can quote the value it refused — and the executor shares
/// its sandbox with tenant code.
fn unreadable(failure: &serde_json::Error) {
    let event = EVENT_MESSAGE_UNREADABLE;
    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let (line, column) = (failure.line(), failure.column());
    tracing::warn!(
        event,
        error_code,
        line,
        column,
        "a message from the executor did not decode"
    );
}
