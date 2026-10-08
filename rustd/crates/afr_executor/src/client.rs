//! The supervisor's end of the executor socket.
//!
//! One link task owns the socket, the calls awaiting an answer and the sender
//! of every open process's events; a [`Client`] encodes its call as a line in
//! its own task, hands it to the link through a channel and waits on a
//! one-shot reply. When the socket closes, every waiting call
//! fails and every open process ends `Interrupted` — once, because the link
//! drops each sender as it ends it.
//!
//! A process's events that go before its ending was read kill it, so a caller
//! that leaves mid-command — a cancelled tool call — leaves nothing running.
//!
//! Every call has a deadline. An executor that stops answering — stopped,
//! wedged, its sandbox frozen — fails the call and gives up the link with it,
//! so the supervisor is never left waiting on a sandbox that will not speak.

use std::borrow::Cow;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use afd_core::clock::saturating_millis;
use backon::{ConstantBuilder, Retryable as _};
use bytes::Bytes;
use serde::Serialize;
use serde_json::value::RawValue;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::api::{Executor, FileContent, Listing, Process, ProcessId, Spawn};
use crate::error::{self, Result};
use crate::protocol::{
    KillParams, MAX_FRAME_BYTES, METHOD_APPEND_FILE, METHOD_DELETE_FILE, METHOD_KILL,
    METHOD_LIST_DIR, METHOD_READ_FILE, METHOD_SPAWN, METHOD_WRITE, METHOD_WRITE_FILE, PathParams,
    ReadParams, ReadResult, SpawnParams, WriteFileParams, WriteParams, decoded, request,
};

mod call;
mod link;

use self::call::{Call, CallIds, Reply};
use self::link::Link;

/// How many calls may wait for the link to send them.
const CALL_BACKLOG: usize = 64;
/// How long a call waits for the executor's answer. Every call is answered
/// promptly by a live executor — a write is answered once queued, a read is
/// capped — so this bounds a stopped one, not a slow one.
pub(crate) const CALL_DEADLINE: Duration = Duration::from_secs(30);
/// The pause between attempts to reach a socket that is not listening yet.
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(10);
/// The longest line a call may be: a frame the executor reads, and its
/// delimiter.
const MAX_LINE_BYTES: usize = MAX_FRAME_BYTES + 1;
/// A call too long for the executor to read, refused before it is sent: the
/// executor would answer it by closing the connection.
const DETAIL_TOO_LONG: &str = "the call is longer than the executor reads";

/// A connection to one sandbox's executor.
///
/// Dropping it closes the connection, which ends every process it started.
#[derive(Debug)]
pub struct Client {
    calls: mpsc::Sender<Call>,
    ids: Arc<CallIds>,
    lost: CancellationToken,
    deadline: Duration,
}

impl Client {
    /// Connects to the executor on `socket`, retrying until it listens or
    /// `deadline` passes.
    ///
    /// The executor binds its socket as it starts, so a supervisor that
    /// starts one connects within the time a start takes rather than at once.
    ///
    /// # Errors
    /// The last attempt's failure once `deadline` passes, or at once for a
    /// failure that waiting cannot fix, such as a refused permission.
    pub async fn connect_within(socket: &Path, deadline: Duration) -> Result<Self> {
        let attempts = deadline.as_millis() / CONNECT_RETRY_DELAY.as_millis();
        let policy = ConstantBuilder::new()
            .with_delay(CONNECT_RETRY_DELAY)
            .with_max_times(usize::try_from(attempts).unwrap_or(usize::MAX));
        let stream = (|| UnixStream::connect(socket))
            .retry(policy)
            .when(not_listening_yet)
            .await?;
        Ok(Self::over(stream, CALL_DEADLINE))
    }

    /// A client over a connected `stream` whose calls wait at most `deadline`.
    pub(crate) fn over(stream: UnixStream, deadline: Duration) -> Self {
        let (calls, queue) = mpsc::channel(CALL_BACKLOG);
        let ids = Arc::new(CallIds::default());
        let lost = CancellationToken::new();
        let outbox = calls.downgrade();
        tokio::spawn(Link::new(stream, queue, outbox, Arc::clone(&ids), lost.clone()).run());
        Self {
            calls,
            ids,
            lost,
            deadline,
        }
    }

    /// Sends one call and waits, until the deadline, for the reply `wrap`
    /// routes back. A call past its deadline gives up the link: an executor
    /// that does not answer one call will not answer the next.
    async fn ask<P: Serialize, T>(
        &self,
        method: &'static str,
        params: &P,
        wrap: impl FnOnce(oneshot::Sender<Result<T>>) -> Reply,
    ) -> Result<T> {
        let id = self.ids.next();
        let line = request(id, method, params)?;
        if line.len() > MAX_LINE_BYTES {
            return Err(error::invalid_params(DETAIL_TOO_LONG));
        }
        let (reply, answer) = oneshot::channel();
        let call = Call {
            id,
            line,
            reply: wrap(reply),
        };
        let answered = tokio::time::timeout(self.deadline, async {
            self.calls
                .send(call)
                .await
                .map_err(|_closed| error::connection_lost())?;
            answer.await.map_err(|_closed| error::connection_lost())?
        })
        .await;
        answered.unwrap_or_else(|_elapsed| {
            self.lost.cancel();
            Err(error::unresponsive(method))
        })
    }

    /// A call whose result is a value, decoded through the object-only gate:
    /// the executor shares its sandbox with tenant code.
    async fn fetch<P: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        method: &'static str,
        params: &P,
    ) -> Result<T> {
        let raw: Box<RawValue> = self.ask(method, params, Reply::Value).await?;
        Ok(decoded(&raw)?)
    }

    /// A call answered with nothing but that it was done.
    async fn order<P: Serialize>(&self, method: &'static str, params: &P) -> Result<()> {
        self.ask(method, params, Reply::Value).await.map(drop)
    }
}

/// Whether a failed connect is an executor that has not bound its socket yet,
/// or has a socket file and is not accepting yet — the two that waiting fixes.
fn not_listening_yet(failure: &io::Error) -> bool {
    matches!(
        failure.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

#[async_trait::async_trait]
impl Executor for Client {
    async fn spawn(&self, spawn: &Spawn) -> Result<Process> {
        let params = SpawnParams {
            argv: Cow::Borrowed(spawn.argv()),
            cwd: spawn.working_directory().map(Cow::Borrowed),
            env: Cow::Borrowed(spawn.environment()),
            pty: spawn.on_terminal(),
            timeout_ms: spawn.time_limit().map(saturating_millis),
        };
        self.ask(METHOD_SPAWN, &params, Reply::Process).await
    }

    async fn write(&self, process: ProcessId, data: Bytes) -> Result<()> {
        let params = WriteParams {
            process_id: process.get(),
            data,
        };
        self.order(METHOD_WRITE, &params).await
    }

    async fn kill(&self, process: ProcessId) -> Result<()> {
        let params = KillParams {
            process_id: process.get(),
        };
        self.order(METHOD_KILL, &params).await
    }

    async fn read_file(&self, path: &str, max_bytes: u64) -> Result<FileContent> {
        let params = ReadParams {
            path: Cow::Borrowed(path),
            max_bytes,
        };
        let read: ReadResult = self.fetch(METHOD_READ_FILE, &params).await?;
        Ok(FileContent {
            data: read.content,
            truncated: read.truncated,
        })
    }

    async fn write_file(&self, path: &str, data: Bytes) -> Result<()> {
        let params = WriteFileParams {
            path: Cow::Borrowed(path),
            content: data,
        };
        self.order(METHOD_WRITE_FILE, &params).await
    }

    async fn append_file(&self, path: &str, data: Bytes) -> Result<()> {
        let params = WriteFileParams {
            path: Cow::Borrowed(path),
            content: data,
        };
        self.order(METHOD_APPEND_FILE, &params).await
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let params = PathParams {
            path: Cow::Borrowed(path),
        };
        self.order(METHOD_DELETE_FILE, &params).await
    }

    async fn list_dir(&self, path: &str) -> Result<Listing> {
        let params = PathParams {
            path: Cow::Borrowed(path),
        };
        self.fetch(METHOD_LIST_DIR, &params).await
    }
}

#[cfg(test)]
#[path = "client/tests.rs"]
mod tests;
