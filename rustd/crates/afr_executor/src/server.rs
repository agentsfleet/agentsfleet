//! The in-sandbox end: one connection, every process it starts, every file call.
//!
//! The session task owns the socket's read half, the table of running
//! processes and the tasks serving calls; the socket's write half belongs to a
//! writer task fed by one channel, so a response and a process's output reach
//! the supervisor in the order they were queued. Nothing here is shared behind
//! a lock: a process is reached through its control channel, and the
//! workspace handle is read-only.

mod files;
mod launch;
mod process;
mod session;

use std::path::Path;
use std::sync::Arc;

use futures_util::SinkExt as _;
use tokio::net::UnixListener;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedWrite, LinesCodec};

use self::files::Workspace;
use self::session::Session;
use crate::error::Result;

/// Where the workspace disk is mounted inside every sandbox.
pub const WORKSPACE_ROOT: &str = "/workspace";

/// The executor bound its socket and waits for the supervisor.
const EVENT_SERVE_STARTED: &str = "executor_serve_started";
/// The supervisor's connection closed and every process it started has ended.
const EVENT_SERVE_COMPLETED: &str = "executor_serve_completed";

/// Serves one supervisor connection on `socket`, confining file calls and
/// working directories to `root`, until the connection closes.
///
/// Every process the connection started has ended when this returns: a
/// connection that closes takes its processes with it.
///
/// # Errors
/// When the socket cannot be bound, `root` cannot be opened, or no connection
/// arrives.
pub async fn serve(socket: &Path, root: &Path) -> Result<()> {
    let listener = UnixListener::bind(socket)?;
    let workspace = Arc::new(Workspace::open(root)?);
    let event = EVENT_SERVE_STARTED;
    tracing::info!(event, "the executor is listening");

    let served = accept(&listener, workspace).await;

    let event = EVENT_SERVE_COMPLETED;
    let ok = served.is_ok();
    tracing::info!(event, ok, "the executor's connection closed");
    served
}

/// Takes the one connection and serves it to its end.
async fn accept(listener: &UnixListener, workspace: Arc<Workspace>) -> Result<()> {
    let (stream, _peer) = listener.accept().await?;
    let (read, write) = stream.into_split();
    let (outbound, queued) = mpsc::unbounded_channel();
    let writer = tokio::spawn(write_lines(
        FramedWrite::new(write, LinesCodec::new()),
        queued,
    ));
    Session::new(workspace, outbound).run(read).await;
    // The session dropped the last sender, so the writer drains and ends.
    let _drained = writer.await;
    Ok(())
}

/// Writes every queued line until the queue closes or the socket does.
async fn write_lines(
    mut sink: FramedWrite<OwnedWriteHalf, LinesCodec>,
    mut queued: mpsc::UnboundedReceiver<String>,
) {
    while let Some(line) = queued.recv().await {
        if sink.send(line).await.is_err() {
            break;
        }
    }
}
