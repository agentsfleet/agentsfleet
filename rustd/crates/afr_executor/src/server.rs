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
//! Binding is split from serving. [`bind`] needs no runtime and no workspace,
//! so the sandbox claims its socket first and then drops the right to create
//! one before any request is read.

mod files;
mod launch;
mod process;
mod session;

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use tokio::io::AsyncWriteExt as _;
use tokio::net::UnixListener;
use tokio::net::unix::OwnedWriteHalf;
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

/// A bound executor socket, not yet serving.
#[derive(Debug)]
pub struct Listener {
    socket: std::os::unix::net::UnixListener,
    /// Where every process it starts is placed: where the executor runs,
    /// unless [`Listener::with_tenant`] names a leaf.
    placement: Arc<dyn Placement>,
}

/// Claims `socket` for the executor.
///
/// Synchronous, and needs no runtime: the sandbox binds before it hardens
/// itself, then serves with what it kept.
///
/// # Errors
/// When `socket` exists already or cannot be created.
pub fn bind(socket: &Path) -> Result<Listener> {
    let socket = std::os::unix::net::UnixListener::bind(socket)?;
    socket.set_nonblocking(true)?;
    Ok(Listener {
        socket,
        placement: Arc::new(Inherit),
    })
}

impl Listener {
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
        tracing::info!(event, "the executor is listening");
        let served = accept(self.socket, self.placement, root).await;
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

/// [`bind`] then [`Listener::serve`], for a caller with nothing to do between.
///
/// # Errors
/// As [`bind`] and [`Listener::serve`].
pub async fn serve(socket: &Path, root: &Path) -> Result<()> {
    bind(socket)?.serve(root).await
}

/// Takes the one connection and serves it to its end.
async fn accept(
    socket: std::os::unix::net::UnixListener,
    placement: Arc<dyn Placement>,
    root: &Path,
) -> Result<()> {
    let listener = UnixListener::from_std(socket)?;
    let workspace = Arc::new(Workspace::open(root)?);
    let (stream, _peer) = listener.accept().await?;
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
#[path = "server/tests.rs"]
mod tests;
