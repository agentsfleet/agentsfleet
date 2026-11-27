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
//! Taking the link is split from serving. [`bind`] needs no runtime and no
//! workspace, so a caller takes its link before it confines itself and before
//! any request is read.

mod files;
mod launch;
mod process;
mod session;

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use self::files::Workspace;
pub use self::launch::{Entry, InnerEnvironment, LeafFreezer, Tenant};
use self::launch::{Inherit, Placement};
use self::session::Session;
use af_guest_wire::AgentStream;
use af_guest_wire::Sizes;
use af_guest_wire::error::Result;

/// Output lines that may wait for the writer, across every process on the
/// connection: 64 pipe reads, about 1 MiB of output. Past it a process's
/// output waits.
const OUTPUT_LINES_IN_FLIGHT: usize = 64;

/// The guest agent took its socket and waits for the supervisor.
const EVENT_SERVE_STARTED: &str = "agent_serve_started";
/// The supervisor's connection closed and every process it started has ended.
const EVENT_SERVE_COMPLETED: &str = "agent_serve_completed";
/// The guest agent could not serve: no workspace, or no connection.
const EVENT_SERVE_FAILED: &str = "agent_serve_failed";

/// Where a guest agent's one supervisor connection comes from.
enum Link {
    /// A bound socket: the connection is the first one accepted.
    Bound(std::os::unix::net::UnixListener),
    /// Any other byte stream the supervisor is already on the far end of: a
    /// virtual machine's vsock connection.
    Streamed(Box<dyn AgentStream>),
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bound(socket) => f.debug_tuple("Bound").field(socket).finish(),
            Self::Streamed(_) => f.write_str("Streamed"),
        }
    }
}

impl Link {
    /// The supervisor's connection, waited for where the socket is bound.
    async fn connection(self) -> Result<Box<dyn AgentStream>> {
        Ok(match self {
            Self::Bound(socket) => Box::new(UnixListener::from_std(socket)?.accept().await?.0),
            Self::Streamed(stream) => stream,
        })
    }
}

/// A guest agent holding its link, not yet serving.
#[derive(Debug)]
pub struct Server {
    link: Link,
    /// Where every process it starts is placed: where the guest agent runs,
    /// unless [`Server::with_tenant`] names a leaf.
    placement: Arc<dyn Placement>,
    /// The environment every process it starts runs in: none, unless
    /// [`Server::with_inner`] names one.
    inner: Option<Arc<dyn InnerEnvironment>>,
    /// The read clamp and the frame cap: the defaults, unless
    /// [`Server::with_sizes`] names its starter's.
    sizes: Sizes,
}

/// Claims `socket` for the guest agent, which serves the first connection.
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

/// Takes `stream`, a connection the supervisor already holds the far end of:
/// a virtual machine's guest agent serves the host's vsock connection this way,
/// the same lines over it.
#[must_use]
pub fn streamed(stream: impl AgentStream + 'static) -> Server {
    Server::over(Link::Streamed(Box::new(stream)))
}

impl Server {
    /// A guest agent on `link` whose processes run where it does.
    fn over(link: Link) -> Self {
        Self {
            link,
            placement: Arc::new(Inherit),
            inner: None,
            sizes: Sizes::DEFAULT,
        }
    }

    /// Starts every process inside `inner`, and renews `inner` when the
    /// supervisor asks: the guest agent stays outside it, as the sandbox's root
    /// service.
    #[must_use]
    pub fn with_inner(self, inner: Arc<dyn InnerEnvironment>) -> Self {
        Self {
            inner: Some(inner),
            ..self
        }
    }

    /// Starts every process in `tenant`'s leaf instead, refusing a spawn
    /// whose process cannot move there. Shared, so every connection a guest
    /// serves in turn places into one leaf and judges by one count.
    #[must_use]
    pub fn with_tenant(self, tenant: Arc<Tenant>) -> Self {
        Self {
            placement: tenant,
            ..self
        }
    }

    /// Clamps every read to `sizes.max_read_bytes` and reads no line longer
    /// than `sizes.max_frame_bytes`: what the runner that started it chose,
    /// and connects its client with.
    #[must_use]
    pub const fn with_sizes(mut self, sizes: Sizes) -> Self {
        self.sizes = sizes;
        self
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
        tracing::info!(event, "the guest agent is waiting for its supervisor");
        let served = accept(self.link, self.placement, self.inner, self.sizes, root).await;
        match &served {
            Ok(()) => {
                let event = EVENT_SERVE_COMPLETED;
                tracing::info!(event, "the guest agent's connection closed");
            }
            Err(failure) => {
                let event = EVENT_SERVE_FAILED;
                let error_code = failure.code().as_str();
                let reason = failure.wire_message();
                tracing::warn!(event, error_code, reason, "the guest agent could not serve");
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
async fn accept(
    link: Link,
    placement: Arc<dyn Placement>,
    inner: Option<Arc<dyn InnerEnvironment>>,
    sizes: Sizes,
    root: &Path,
) -> Result<()> {
    let workspace = Arc::new(Workspace::open(root)?);
    let stream = link.connection().await?;
    let (read, write) = tokio::io::split(stream);
    let (answers, answered) = mpsc::unbounded_channel();
    let (output, said) = mpsc::channel(OUTPUT_LINES_IN_FLIGHT);
    let writer = tokio::spawn(write_lines(write, answered, said));
    Session::new(workspace, placement, inner, answers, output, sizes)
        .run(read)
        .await;
    // The session dropped the last senders, so the writer drains and ends.
    Ok(writer.await?)
}

/// Writes every queued line until both queues close or the socket does.
/// Answers go first: one queued before a line of output is written before it.
/// Each line is already delimited, so it goes to the socket as it is.
async fn write_lines(
    mut write: impl AsyncWrite + Unpin,
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
mod host_tests;
#[cfg(test)]
mod pin_tests;
#[cfg(test)]
mod tests;
