//! The supervisor's end of the executor link.
//!
//! One link task owns the stream, the calls awaiting an answer and the sender
//! of every open process's events; a [`Client`] encodes its call as a line in
//! its own task, hands it to the link through a channel and waits on a
//! one-shot reply. When the stream closes, every waiting call
//! fails and every open process ends `Interrupted` — once, because the link
//! drops each sender as it ends it.
//!
//! The stream is whatever the client's [`Transport`] dials. A call that finds
//! the link closed dials once more and goes over the new link; a call already
//! sent is never sent again, since the executor may have run it. Processes
//! the old link carried stay `Interrupted`: the executor that ran them ended
//! them with its connection.
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

use af_core::clock::saturating_millis;
use backon::{ConstantBuilder, Retryable as _};
use bytes::Bytes;
use serde::Serialize;
use serde_json::value::RawValue;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio::sync::{Mutex, oneshot};

use crate::api::{
    Executor, FileContent, Found, Imported, Listing, Process, ProcessId, Search, Spawn,
};
use crate::error::{self, Result};
use crate::protocol::{
    KillParams, METHOD_DELETE_FILE, METHOD_KILL, METHOD_LIST_DIR, METHOD_READ_FILE,
    METHOD_RECOVERY_IDLE, METHOD_SEARCH, METHOD_SPAWN, METHOD_WRITE, METHOD_WRITE_FILE, PathParams,
    ReadParams, ReadResult, RecoveryParams, RecoveryProof, SpawnParams, WriteFileParams,
    WriteParams, decoded, request,
};
use crate::sizes::Sizes;
use crate::timing::CALL_DEADLINE;

mod call;
mod link;
pub mod transport;
mod tree;

use self::call::{Call, CallIds, Reply};
use self::link::{Linked, start};
use self::transport::{Stream, Transport};

/// The pause between attempts to reach a socket that is not listening yet.
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(10);
/// A call too long for the executor to read, refused before it is sent: the
/// executor would answer it by closing the connection.
const DETAIL_TOO_LONG: &str = "the call is longer than the executor reads";

/// A connection to one sandbox's executor.
///
/// Dropping it closes the connection, which ends every process it started.
#[derive(Debug)]
pub struct Client {
    /// What dials a link again when a call finds it closed; none for a link
    /// handed over connected, whose one stream was that one.
    transport: Option<Box<dyn Transport>>,
    /// The link calls go over now; replaced when a call finds it closed.
    link: Mutex<Linked>,
    ids: Arc<CallIds>,
    deadline: Duration,
    /// What both ends of the link hold to: the executor was started with the
    /// same.
    sizes: Sizes,
}

impl Client {
    /// Connects to the executor on `socket`, retrying until it listens or
    /// `deadline` passes, holding the link to the `sizes` the executor was
    /// started with.
    ///
    /// The executor binds its socket as it starts, so a supervisor that
    /// starts one connects within the time a start takes rather than at once.
    ///
    /// # Errors
    /// The last attempt's failure once `deadline` passes, or at once for a
    /// failure that waiting cannot fix, such as a refused permission.
    pub async fn connect_within(socket: &Path, deadline: Duration, sizes: Sizes) -> Result<Self> {
        let attempts = deadline
            .as_millis()
            .div_euclid(CONNECT_RETRY_DELAY.as_millis());
        let policy = ConstantBuilder::new()
            .with_delay(CONNECT_RETRY_DELAY)
            .with_max_times(usize::try_from(attempts).unwrap_or(usize::MAX));
        let stream = (|| UnixStream::connect(socket))
            .retry(policy)
            .when(not_listening_yet)
            .await?;
        Ok(Self::over(stream, CALL_DEADLINE, sizes))
    }

    /// A client over `stream`, already connected to an executor: the engine's
    /// end of the socket pair its sandbox inherited, started with `sizes`.
    /// Nothing is retried, as nothing has to bind first, and a link that
    /// closes is not dialled again: its one stream was this one.
    ///
    /// # Errors
    /// The stream cannot be made non-blocking or join the runtime.
    pub fn connected(stream: std::os::unix::net::UnixStream, sizes: Sizes) -> Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self::over(
            UnixStream::from_std(stream)?,
            CALL_DEADLINE,
            sizes,
        ))
    }

    /// A client over whatever `transport` dials, dialled once now and again
    /// whenever a call finds the link closed, holding the link to the `sizes`
    /// the executor was started with.
    ///
    /// # Errors
    /// The first dial fails.
    pub async fn dial(transport: impl Transport + 'static, sizes: Sizes) -> Result<Self> {
        let stream = transport.dial().await?;
        Ok(Self::linked(
            stream,
            Some(Box::new(transport)),
            CALL_DEADLINE,
            sizes,
        ))
    }

    /// A client over a connected `stream` whose calls wait at most `deadline`,
    /// which dials no second link: once it closes, every call fails as the
    /// connection closed.
    pub(crate) fn over(stream: UnixStream, deadline: Duration, sizes: Sizes) -> Self {
        Self::linked(Box::new(stream), None, deadline, sizes)
    }

    /// A client whose first link runs over `stream`, and whose later ones
    /// over what `transport` dials.
    fn linked(
        stream: Box<dyn Stream>,
        transport: Option<Box<dyn Transport>>,
        deadline: Duration,
        sizes: Sizes,
    ) -> Self {
        let ids = Arc::new(CallIds::default());
        let link = Mutex::new(start(stream, &ids, sizes));
        Self {
            transport,
            link,
            ids,
            deadline,
            sizes,
        }
    }

    /// The link a call goes over: the current one, or, when it was found
    /// closed, one dialled now. Callers that find it closed together dial
    /// once between them, as the first holds the lock while it dials.
    async fn live(&self) -> Result<Linked> {
        let mut current = self.link.lock().await;
        if !current.is_open() {
            let transport = self.transport.as_ref().ok_or_else(error::connection_lost)?;
            let stream = transport.dial().await.map_err(error::redial_failed)?;
            *current = start(stream, &self.ids, self.sizes);
        }
        Ok(current.clone())
    }

    /// Sends one call and waits, until the deadline, for the reply `wrap`
    /// routes back. A call past its deadline gives up the link: an executor
    /// that does not answer one call will not answer the next.
    async fn ask<P: Serialize + Sync, T: Send>(
        &self,
        method: &'static str,
        params: &P,
        wrap: impl FnOnce(oneshot::Sender<Result<T>>) -> Reply + Send,
    ) -> Result<T> {
        let id = self.ids.next();
        let line = request(id, method, params)?;
        // A frame the executor reads, and its delimiter.
        if line.len() > self.sizes.max_frame_bytes.saturating_add(1) {
            return Err(error::invalid_params(DETAIL_TOO_LONG));
        }
        let Linked { calls, lost } = self.live().await?;
        let (reply, answer) = oneshot::channel();
        let call = Call {
            id,
            line,
            reply: wrap(reply),
        };
        let answered = tokio::time::timeout(self.deadline, async {
            tokio::select! {
                biased;
                () = lost.cancelled() => Err(error::connection_lost()),
                result = async {
                    calls.send(call).await.map_err(|_closed| error::connection_lost())?;
                    answer.await.map_err(|_closed| error::connection_lost())?
                } => result,
            }
        })
        .await;
        answered.unwrap_or_else(|_elapsed| {
            lost.cancel();
            Err(error::unresponsive(method))
        })
    }

    /// A call whose result is a value, decoded through the object-only gate:
    /// the executor shares its sandbox with tenant code.
    async fn fetch<P: Serialize + Sync, T: serde::de::DeserializeOwned + Send>(
        &self,
        method: &'static str,
        params: &P,
    ) -> Result<T> {
        let raw: Box<RawValue> = self.ask(method, params, Reply::Value).await?;
        Ok(decoded(&raw)?)
    }

    /// A call answered with nothing but that it was done.
    async fn order<P: Serialize + Sync>(&self, method: &'static str, params: &P) -> Result<()> {
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
    fn sizes(&self) -> Sizes {
        self.sizes
    }

    async fn recovery_idle(&self) -> Result<bool> {
        self.fetch(
            METHOD_RECOVERY_IDLE,
            &RecoveryParams {
                proof: RecoveryProof::FileJobs,
            },
        )
        .await
    }

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

    async fn search(&self, search: &Search) -> Result<Found> {
        self.fetch(METHOD_SEARCH, search).await
    }

    async fn import(
        &self,
        at: &str,
        tree: &mut (dyn AsyncRead + Send + Unpin),
    ) -> Result<Imported> {
        self.import_tree(at, tree).await
    }

    async fn export(
        &self,
        paths: &[&str],
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> Result<()> {
        self.export_tree(paths, into).await
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod invalidation_tests;

#[cfg(test)]
mod redial_tests;
