//! The in-sandbox end: one connection, every process it starts, every file call.
//!
//! The session task owns the socket's read half, the table of running
//! processes and the tasks serving calls; the socket's write half belongs to a
//! writer task fed by two queues. Answers go unbounded and first, so a spawn's
//! answer reaches the supervisor before the process's first output. Output
//! and each process's exit go through one bounded queue, so they arrive in the
//! order they were said, and a process that writes faster than the supervisor
//! reads waits on its own full pipe. Nothing here is shared behind a lock: a
//! process is reached through its input queue and its stop token, and the
//! workspace handle is read-only.
//!
//! Taking the link is split from serving. [`bind`] and [`connected`] need no
//! runtime and no workspace, so the sandbox takes its link before it confines
//! itself and before any request is read.

mod files;
mod launch;
mod process;
mod session;

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use tokio::io::AsyncWriteExt as _;
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use self::files::Workspace;
pub use self::launch::Tenant;
use self::launch::{Inherit, Placement};
use self::session::Session;
use crate::error::Result;

/// Where the workspace disk is mounted inside every sandbox.
pub const WORKSPACE_ROOT: &str = "/workspace";
/// Output lines that may wait for the writer, across every process on the
/// connection: 64 pipe reads, about 1 MiB of output. Past it a process's
/// output waits, as Codex's exec-server waits on its bounded notification
/// queue (`exec-server/src/local_process.rs`, `NOTIFICATION_CHANNEL_CAPACITY`).
const OUTPUT_LINES_IN_FLIGHT: usize = 64;

/// The executor took its socket and waits for the supervisor.
const EVENT_SERVE_STARTED: &str = "executor_serve_started";
/// The supervisor's connection closed and every process it started has ended.
const EVENT_SERVE_COMPLETED: &str = "executor_serve_completed";
/// The executor could not serve: no workspace, or no connection.
const EVENT_SERVE_FAILED: &str = "executor_serve_failed";

/// Where an executor's one supervisor connection comes from.
#[derive(Debug)]
enum Link {
    /// A bound socket: the connection is the first one accepted.
    Bound(std::os::unix::net::UnixListener),
    /// A stream whose other end the supervisor already holds.
    Connected(std::os::unix::net::UnixStream),
}

impl Link {
    /// The supervisor's connection, waited for where the socket is bound.
    async fn connection(self) -> Result<UnixStream> {
        Ok(match self {
            Self::Bound(socket) => UnixListener::from_std(socket)?.accept().await?.0,
            Self::Connected(stream) => UnixStream::from_std(stream)?,
        })
    }
}

/// An executor holding its link, not yet serving.
#[derive(Debug)]
pub struct Server {
    link: Link,
    /// Where every process it starts is placed: where the executor runs,
    /// unless [`Server::with_tenant`] names a leaf.
    placement: Arc<dyn Placement>,
}

/// Claims `socket` for the executor, which serves the first connection.
///
/// Synchronous, and needs no runtime: a caller binds before it hardens
/// itself, then serves with what it kept.
///
/// # Errors
/// When `socket` exists already or cannot be created.
pub fn bind(socket: &Path) -> Result<Server> {
    let socket = std::os::unix::net::UnixListener::bind(socket)?;
    socket.set_nonblocking(true)?;
    Ok(Server::over(Link::Bound(socket)))
}

/// Takes `stream`, already connected to the supervisor: the end of a socket
/// pair the sandbox inherited, which no path names and nothing else reaches.
///
/// Synchronous and needs no runtime, as [`bind`] does.
///
/// # Errors
/// When the stream cannot be made non-blocking.
pub fn connected(stream: std::os::unix::net::UnixStream) -> Result<Server> {
    stream.set_nonblocking(true)?;
    Ok(Server::over(Link::Connected(stream)))
}

impl Server {
    /// An executor on `link` whose processes run where it does.
    fn over(link: Link) -> Self {
        Self {
            link,
            placement: Arc::new(Inherit),
        }
    }

    /// Starts every process in `tenant`'s leaf instead, refusing a spawn
    /// whose process cannot move there.
    #[must_use]
    pub fn with_tenant(self, tenant: Tenant) -> Self {
        Self {
            placement: Arc::new(tenant),
            ..self
        }
    }

    /// Serves one supervisor connection, confining file calls and working
    /// directories to `root`, until the connection closes.
    ///
    /// Every process the connection started has ended when this returns: a
    /// connection that closes takes its processes with it.
    ///
    /// # Errors
    /// When `root` cannot be opened, or no connection arrives.
    pub async fn serve(self, root: &Path) -> Result<()> {
        let event = EVENT_SERVE_STARTED;
        tracing::info!(event, "the executor is waiting for its supervisor");
        let served = accept(self.link, self.placement, root).await;
        match &served {
            Ok(()) => {
                let event = EVENT_SERVE_COMPLETED;
                tracing::info!(event, "the executor's connection closed");
            }
            Err(failure) => {
                let event = EVENT_SERVE_FAILED;
                let error_code = failure.code().as_str();
                let reason = failure.wire_message();
                tracing::warn!(event, error_code, reason, "the executor could not serve");
            }
        }
        served
    }
}

/// [`bind`] then [`Server::serve`], for a caller with nothing to do between.
///
/// # Errors
/// As [`bind`] and [`Server::serve`].
pub async fn serve(socket: &Path, root: &Path) -> Result<()> {
    bind(socket)?.serve(root).await
}

/// Takes the one connection and serves it to its end.
async fn accept(link: Link, placement: Arc<dyn Placement>, root: &Path) -> Result<()> {
    let workspace = Arc::new(Workspace::open(root)?);
    let stream = link.connection().await?;
    let (read, write) = stream.into_split();
    let (answers, answered) = mpsc::unbounded_channel();
    let (output, said) = mpsc::channel(OUTPUT_LINES_IN_FLIGHT);
    let writer = tokio::spawn(write_lines(write, answered, said));
    Session::new(workspace, placement, answers, output)
        .run(read)
        .await;
    // The session dropped the last senders, so the writer drains and ends.
    Ok(writer.await?)
}

/// Writes every queued line until both queues close or the socket does.
/// Answers go first: one queued before a line of output is written before it.
/// Each line is already delimited, so it goes to the socket as it is.
async fn write_lines(
    mut write: OwnedWriteHalf,
    mut answers: mpsc::UnboundedReceiver<Bytes>,
    mut output: mpsc::Receiver<Bytes>,
) {
    loop {
        let line = tokio::select! {
            biased;
            Some(line) = answers.recv() => line,
            Some(line) = output.recv() => line,
            else => break,
        };
        if write.write_all(&line).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
