//! The supervisor's end of the executor socket.
//!
//! One link task owns the socket, the calls awaiting an answer and the sender
//! of every open process's events; a [`Client`] reaches it through a channel
//! and waits on a one-shot reply. When the socket closes, every waiting call
//! fails and every open process ends `Interrupted` — once, because the link
//! drops each sender as it ends it.

use std::borrow::Cow;
use std::path::Path;

use bytes::Bytes;
use serde::Serialize;
use serde_json::value::RawValue;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

use crate::api::{DirEntry, Executor, FileContent, Process, ProcessId, Spawn};
use crate::error::{self, Result};
use crate::protocol::{
    KillParams, ListParams, ListResult, METHOD_KILL, METHOD_LIST_DIR, METHOD_READ_FILE,
    METHOD_SPAWN, METHOD_WRITE, METHOD_WRITE_FILE, ReadParams, ReadResult, SpawnParams,
    WriteFileParams, WriteParams, decode, encode,
};

mod link;

use self::link::{Call, Link, Reply};

/// How many calls may wait for the link to send them.
const CALL_BACKLOG: usize = 64;

/// A connection to one sandbox's executor.
///
/// Dropping it closes the connection, which ends every process it started.
#[derive(Debug)]
pub struct Client {
    calls: mpsc::Sender<Call>,
}

impl Client {
    /// Connects to the executor listening on `socket`.
    ///
    /// # Errors
    /// When nothing accepts on `socket`.
    pub async fn connect(socket: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket).await?;
        let (calls, queue) = mpsc::channel(CALL_BACKLOG);
        tokio::spawn(Link::new(stream, queue).run());
        Ok(Self { calls })
    }

    /// Sends one call and waits for the reply `wrap` routes back.
    async fn ask<P: Serialize, T>(
        &self,
        method: &'static str,
        params: &P,
        wrap: impl FnOnce(oneshot::Sender<Result<T>>) -> Reply,
    ) -> Result<T> {
        let params = serde_json::value::to_raw_value(params)?;
        let (reply, answer) = oneshot::channel();
        self.calls
            .send(Call {
                method,
                params,
                reply: wrap(reply),
            })
            .await
            .map_err(|_closed| error::connection_lost())?;
        answer.await.map_err(|_closed| error::connection_lost())?
    }

    /// A call whose result is a value, decoded through the object-only gate:
    /// the executor shares its sandbox with tenant code.
    async fn fetch<P: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        method: &'static str,
        params: &P,
    ) -> Result<T> {
        let raw: Box<RawValue> = self.ask(method, params, Reply::Value).await?;
        Ok(afd_core::json::object_from_slice(raw.get().as_bytes())?)
    }
}

#[async_trait::async_trait]
impl Executor for Client {
    async fn spawn(&self, spawn: Spawn) -> Result<Process> {
        let params = SpawnParams {
            argv: Cow::Borrowed(spawn.argv()),
            cwd: spawn.working_directory().map(Cow::Borrowed),
            env: Cow::Borrowed(spawn.environment()),
            pty: spawn.on_terminal(),
            timeout_ms: spawn
                .time_limit()
                .map(|limit| u64::try_from(limit.as_millis()).unwrap_or(u64::MAX)),
        };
        self.ask(METHOD_SPAWN, &params, Reply::Process).await
    }

    async fn write(&self, process: ProcessId, data: Bytes) -> Result<()> {
        let params = WriteParams {
            process_id: process.get(),
            data: encode(&data),
        };
        self.ask(METHOD_WRITE, &params, Reply::Value)
            .await
            .map(drop)
    }

    async fn kill(&self, process: ProcessId) -> Result<()> {
        let params = KillParams {
            process_id: process.get(),
        };
        self.ask(METHOD_KILL, &params, Reply::Value).await.map(drop)
    }

    async fn read_file(&self, path: &str, max_bytes: u64) -> Result<FileContent> {
        let params = ReadParams {
            path: Cow::Borrowed(path),
            max_bytes,
        };
        let read: ReadResult = self.fetch(METHOD_READ_FILE, &params).await?;
        Ok(FileContent {
            data: decode(&read.content)?,
            truncated: read.truncated,
        })
    }

    async fn write_file(&self, path: &str, data: Bytes) -> Result<()> {
        let params = WriteFileParams {
            path: Cow::Borrowed(path),
            content: encode(&data),
        };
        self.ask(METHOD_WRITE_FILE, &params, Reply::Value)
            .await
            .map(drop)
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let params = ListParams {
            path: Cow::Borrowed(path),
        };
        let listed: ListResult = self.fetch(METHOD_LIST_DIR, &params).await?;
        Ok(listed
            .entries
            .into_iter()
            .map(|entry| DirEntry {
                name: entry.name,
                kind: entry.kind.into(),
                size: entry.size,
            })
            .collect())
    }
}
