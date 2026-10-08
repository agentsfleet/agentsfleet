//! Starting a process: on pipes, or on a pseudo-terminal.
//!
//! The two differ only in how a process is started, written to and read; once
//! started, both are a [`Spawned`] — a group leader's pid, an exit, an output
//! channel and the tasks feeding it — and the same task drives either. Each
//! launcher takes the queue of writes for its process and drains it into the
//! process's input itself, in order. Both place the process through the
//! session's [`Placement`] before it execs, so a sandbox's tenant processes
//! never share the executor's cgroup.

use std::collections::BTreeMap;
use std::io;
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt as _;
use rustix::process::Pid;
use tokio::io::{AsyncRead, AsyncWriteExt as _};
use tokio::process::ChildStdin;
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;
use tokio_util::task::AbortOnDropHandle;

use super::files::Workspace;
use crate::api::{Ending, Stream};
use crate::edges::Chunk;
use crate::error::{self, Result};
use crate::protocol::{READ_CHUNK_BYTES, SpawnParams};

/// Chunks of output that may wait for the task forwarding them; past this the
/// reader stops reading and the process blocks on its own writes. At one
/// read each, a process that outruns its forwarder holds at most a mebibyte.
pub(super) const OUTPUT_BACKLOG: usize = 64;
/// The variable a program is looked up through.
const PATH_VARIABLE: &str = "PATH";
/// The search path a process gets when its environment names none.
const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
/// A process needs a program to run.
const DETAIL_NO_PROGRAM: &str = "argv must name a program";

/// When a process's leader ends, and how.
pub(super) type Exit = Pin<Box<dyn Future<Output = Ending> + Send>>;

/// How a waited-for process ended. A wait that failed leaves the ending
/// unknown, which is what [`Ending::Interrupted`] says.
pub(super) fn ending_of(waited: io::Result<ExitStatus>) -> Ending {
    waited.map_or(Ending::Interrupted, |status| {
        status.signal().map_or_else(
            || status.code().map_or(Ending::Interrupted, Ending::Exited),
            Ending::Signaled,
        )
    })
}

/// What to start, checked against the workspace.
#[derive(Debug)]
pub(super) struct Plan {
    program: String,
    arguments: Vec<String>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
    terminal: bool,
    timeout: Option<Duration>,
}

impl Plan {
    /// Checks a spawn request: a program, a working directory inside the
    /// workspace, and a search path even when the caller gave none.
    pub(super) fn new(params: SpawnParams<'static>, workspace: &Workspace) -> Result<Self> {
        let mut argv = params.argv.into_owned().into_iter();
        let program = argv
            .next()
            .ok_or_else(|| error::invalid_params(DETAIL_NO_PROGRAM))?;
        let mut env = params.env.into_owned();
        if !env.contains_key(PATH_VARIABLE) {
            env.insert(PATH_VARIABLE.to_owned(), DEFAULT_PATH.to_owned());
        }
        Ok(Self {
            program,
            arguments: argv.collect(),
            cwd: workspace.directory(params.cwd.as_deref())?,
            env,
            terminal: params.pty,
            timeout: params.timeout_ms.map(Duration::from_millis),
        })
    }

    /// Whether it runs on a pseudo-terminal.
    pub(super) const fn on_terminal(&self) -> bool {
        self.terminal
    }

    /// When it is killed, if ever.
    pub(super) const fn time_limit(&self) -> Option<Duration> {
        self.timeout
    }
}

/// A started process.
pub(super) struct Spawned {
    /// The group leader, which is also the group.
    pub(super) pid: Pid,
    /// Resolves when the leader ends.
    pub(super) exit: Exit,
    /// Output as it is read, closed once every reader reaches its end.
    pub(super) output: mpsc::Receiver<Chunk>,
    /// The tasks reading output and writing input, stopped when this is
    /// dropped: a descendant that left the group may hold a pipe open long
    /// after the leader ends.
    pub(super) tasks: Vec<AbortOnDropHandle<()>>,
}

/// Starts processes one way.
pub(super) trait Launcher: Send + Sync {
    /// Starts `plan` as the leader of a new process group, placed by
    /// `placement` before it execs, writing what arrives on `input` to it
    /// until the queue closes or a write fails.
    fn launch(
        &self,
        plan: &Plan,
        placement: &Arc<dyn Placement>,
        input: mpsc::Receiver<Bytes>,
    ) -> Result<Spawned>;
}

mod tenant;
mod terminal;

pub use self::tenant::Tenant;
pub(crate) use self::tenant::{Inherit, Placement};
use self::terminal::Terminal;

/// The launcher for `terminal`.
pub(super) fn launcher(terminal: bool) -> &'static dyn Launcher {
    if terminal { &Terminal } else { &Pipes }
}

/// Standard input, output and error as three pipes.
struct Pipes;

impl Launcher for Pipes {
    fn launch(
        &self,
        plan: &Plan,
        placement: &Arc<dyn Placement>,
        input: mpsc::Receiver<Bytes>,
    ) -> Result<Spawned> {
        let mut command = tokio::process::Command::new(&plan.program);
        command
            .args(&plan.arguments)
            .env_clear()
            .envs(&plan.env)
            .current_dir(&plan.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            // A session dropped without stopping its processes still ends
            // their leaders; a stop reaches the rest of the group.
            .kill_on_drop(true);
        tenant::place(command.as_std_mut(), placement)?;
        let mut child = command.spawn()?;
        let pid = leader(child.id())?;
        let (sender, output) = mpsc::channel(OUTPUT_BACKLOG);
        let (stdin, stdout, stderr) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (stdin, stdout, stderr) = stdin
            .zip(stdout)
            .zip(stderr)
            .map(|((i, o), e)| (i, o, e))
            .ok_or_else(error::launch_incomplete)?;
        let tasks = vec![
            AbortOnDropHandle::new(tokio::spawn(pump(stdout, Stream::Stdout, sender.clone()))),
            AbortOnDropHandle::new(tokio::spawn(pump(stderr, Stream::Stderr, sender))),
            AbortOnDropHandle::new(tokio::spawn(feed(stdin, input))),
        ];
        let exit = Box::pin(async move { ending_of(child.wait().await) });
        Ok(Spawned {
            pid,
            exit,
            output,
            tasks,
        })
    }
}

/// Forwards one pipe's output until it closes or its reader is gone.
async fn pump(reader: impl AsyncRead + Unpin, stream: Stream, sender: mpsc::Sender<Chunk>) {
    let mut chunks = ReaderStream::with_capacity(reader, READ_CHUNK_BYTES);
    while let Some(Ok(data)) = chunks.next().await {
        if sender.send(Chunk { stream, data }).await.is_err() {
            break;
        }
    }
}

/// Writes queued input to a process's standard input, in order, until the
/// queue closes or a write fails; then the queue closes, and later writes are
/// refused.
async fn feed(mut stdin: ChildStdin, mut queued: mpsc::Receiver<Bytes>) {
    while let Some(data) = queued.recv().await {
        if stdin.write_all(&data).await.is_err() || stdin.flush().await.is_err() {
            break;
        }
    }
}

/// The group a just-started leader leads.
fn leader(id: Option<u32>) -> Result<Pid> {
    id.and_then(|raw| i32::try_from(raw).ok())
        .and_then(Pid::from_raw)
        .ok_or_else(error::launch_incomplete)
}

#[cfg(test)]
#[path = "launch/tests.rs"]
mod tests;
