//! The supervisor's end of the guest agent link.
//!
//! One link task owns the stream, the calls awaiting an answer and the sender
//! of every open process's events; a [`Client`] encodes its call as a line in
//! its own task, hands it to the link through a channel and waits on a
//! one-shot reply. When the stream closes, every waiting call
//! fails and every open process ends `Interrupted` — once, because the link
//! drops each sender as it ends it.
//!
//! The stream is whatever the client's [`AgentRoute`] dials. A call that finds
//! the link closed dials once more and goes over the new link; a call already
//! sent is never sent again, since the guest agent may have run it. Processes
//! the old link carried stay `Interrupted`: the guest agent that ran them ended
//! them with its connection.
//!
//! A process's events that go before its ending was read kill it, so a caller
//! that leaves mid-command — a cancelled tool call — leaves nothing running.
//!
//! Every call has a deadline. A guest agent that stops answering — stopped,
//! wedged, its sandbox frozen — fails the call and gives up the link with it,
//! so the supervisor is never left waiting on a sandbox that will not speak.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use af_core::clock::saturating_millis;
use bytes::Bytes;
use serde::Serialize;
use serde_json::value::RawValue;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio::sync::{Mutex, oneshot};

use crate::api::{GuestAgent, Process, ProcessId, Spawn};
use af_guest_wire::Sizes;
use af_guest_wire::error::{self, Result};
use af_guest_wire::protocol::{
    CountParams, Counted, HostsParams, KillParams, Leaf, LeafParams, METHOD_COUNT,
    METHOD_DELETE_FILE, METHOD_FREEZE, METHOD_HOSTS, METHOD_KILL, METHOD_LIST_DIR,
    METHOD_READ_FILE, METHOD_RECOVERY_IDLE, METHOD_RENEW, METHOD_SEARCH, METHOD_SPAWN, METHOD_THAW,
    METHOD_WRITE, METHOD_WRITE_FILE, PathParams, ReadParams, ReadResult, RecoveryParams,
    RecoveryProof, RenewParams, RenewedWorkspace, SpawnParams, WriteFileParams, WriteParams,
    decoded, request,
};
use af_guest_wire::{FileContent, Found, Imported, Listing, Search};

mod call;
mod connect;
mod link;
pub mod route;
mod tree;

use self::call::{Call, CallIds, Reply};
use self::link::{Linked, start};
use self::route::AgentRoute;
use af_guest_wire::AgentStream;

/// A call too long for the guest agent to read, refused before it is sent: the
/// guest agent would answer it by closing the connection.
const DETAIL_TOO_LONG: &str = "the call is longer than the guest agent reads";

/// A connection to one sandbox's guest agent.
///
/// Dropping it closes the connection, which ends every process it started.
#[derive(Debug)]
pub struct Client {
    /// What dials a link again when a call finds it closed; none for a link
    /// handed over connected, whose one stream was that one.
    route: Option<Box<dyn AgentRoute>>,
    /// The link calls go over now; replaced when a call finds it closed.
    link: Mutex<Linked>,
    ids: Arc<CallIds>,
    deadline: Duration,
    /// What both ends of the link hold to: the guest agent was started with the
    /// same.
    sizes: Sizes,
}

impl Client {
    /// A client over a connected `stream` whose calls wait at most `deadline`,
    /// which dials no second link: once it closes, every call fails as the
    /// connection closed.
    pub(crate) fn over(stream: UnixStream, deadline: Duration, sizes: Sizes) -> Self {
        Self::linked(Box::new(stream), None, deadline, sizes)
    }

    /// A client whose first link runs over `stream`, and whose later ones
    /// over what `route` dials.
    fn linked(
        stream: Box<dyn AgentStream>,
        route: Option<Box<dyn AgentRoute>>,
        deadline: Duration,
        sizes: Sizes,
    ) -> Self {
        let ids = Arc::new(CallIds::default());
        let link = Mutex::new(start(stream, &ids, sizes));
        Self {
            route,
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
            let route = self.route.as_ref().ok_or_else(error::connection_lost)?;
            let stream = route.dial().await.map_err(error::redial_failed)?;
            *current = start(stream, &self.ids, self.sizes);
        }
        Ok(current.clone())
    }

    /// Sends one call and waits, until the deadline, for the reply `wrap`
    /// routes back. A call past its deadline gives up the link: a guest agent
    /// that does not answer one call will not answer the next.
    async fn ask<P: Serialize + Sync, T: Send>(
        &self,
        method: &'static str,
        params: &P,
        wrap: impl FnOnce(oneshot::Sender<Result<T>>) -> Reply + Send,
    ) -> Result<T> {
        let id = self.ids.next();
        let line = request(id, method, params)?;
        // A frame the guest agent reads, and its delimiter.
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
    /// the guest agent shares its sandbox with tenant code.
    async fn fetch<P: Serialize + Sync, T: serde::de::DeserializeOwned + Send>(
        &self,
        method: &'static str,
        params: &P,
    ) -> Result<T> {
        let raw: Box<RawValue> = self.ask(method, params, Reply::Value).await?;
        Ok(decoded(&raw)?)
    }

    /// Replaces the hosts file every process inside the sandbox's inner
    /// environment resolves names through by `file`, whole; answers once
    /// every lookup there reads it. The frame is the only bound on `file`.
    ///
    /// # Errors
    /// The guest agent builds no inner environment, or the file could not be put
    /// in place, and the old one stays.
    pub async fn hosts(&self, file: &str) -> Result<()> {
        let params = HostsParams {
            file: Cow::Borrowed(file),
        };
        self.order(METHOD_HOSTS, &params).await
    }

    /// A call answered with nothing but that it was done.
    async fn order<P: Serialize + Sync>(&self, method: &'static str, params: &P) -> Result<()> {
        self.ask(method, params, Reply::Value).await.map(drop)
    }
}

#[async_trait::async_trait]
impl GuestAgent for Client {
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

    async fn calls_idle(&self) -> Result<bool> {
        let params = RecoveryParams {
            proof: RecoveryProof::Calls,
        };
        self.fetch(METHOD_RECOVERY_IDLE, &params).await
    }

    async fn freeze_tenant(&self) -> Result<()> {
        self.order(METHOD_FREEZE, &LeafParams { leaf: Leaf::Tenant })
            .await
    }

    async fn thaw_tenant(&self) -> Result<()> {
        self.order(METHOD_THAW, &LeafParams { leaf: Leaf::Tenant })
            .await
    }

    async fn pids_refused(&self) -> Result<u64> {
        self.fetch(
            METHOD_COUNT,
            &CountParams {
                count: Counted::PidsRefused,
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

    async fn renew(&self) -> Result<()> {
        let params = RenewParams {
            workspace: RenewedWorkspace::Emptied,
        };
        self.order(METHOD_RENEW, &params).await
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod invalidation_tests;

#[cfg(test)]
mod redial_tests;
