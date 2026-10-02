//! Starting a process: on pipes, or on a pseudo-terminal.
//!
//! The two differ only in how a process is started, written to and read; once
//! started, both are a [`Spawned`] — a group leader's pid, an input, an exit
//! and an output channel — and the same task drives either.

use std::collections::BTreeMap;
use std::io;
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt as _;
use rustix::process::Pid;
use tokio::io::{AsyncRead, AsyncWriteExt as _};
use tokio::process::ChildStdin;
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;

use super::files::Workspace;
use crate::api::Stream;
use crate::edges::Chunk;
use crate::error::{self, Result};
use crate::protocol::SpawnParams;

/// Chunks of output that may wait for the task forwarding them; past this the
/// reader stops reading and the process blocks on its own writes.
const OUTPUT_BACKLOG: usize = 64;
/// The variable a program is looked up through.
const PATH_VARIABLE: &str = "PATH";
/// The search path a process gets when its environment names none.
const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
/// A process needs a program to run.
const DETAIL_NO_PROGRAM: &str = "argv must name a program";

/// When a process ends, and how.
pub(super) type Exit = Pin<Box<dyn Future<Output = Status> + Send>>;

/// How a process ended, as the operating system reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    /// It exited with this code.
    Code(i32),
    /// This signal ended it.
    Signal(i32),
    /// Waiting for it failed, so how it ended is unknown.
    Lost,
}

impl Status {
    /// The exit code, when it exited.
    pub(super) const fn code(self) -> Option<i32> {
        match self {
            Self::Code(code) => Some(code),
            Self::Signal(_) | Self::Lost => None,
        }
    }

    /// The signal, when one ended it.
    pub(super) const fn signal(self) -> Option<i32> {
        match self {
            Self::Signal(signal) => Some(signal),
            Self::Code(_) | Self::Lost => None,
        }
    }
}

impl From<std::process::ExitStatus> for Status {
    fn from(status: std::process::ExitStatus) -> Self {
        status.signal().map_or_else(
            || status.code().map_or(Self::Lost, Self::Code),
            Self::Signal,
        )
    }
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
        env.entry(PATH_VARIABLE.to_owned())
            .or_insert_with(|| DEFAULT_PATH.to_owned());
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
    /// Where writes go.
    pub(super) input: Box<dyn Input>,
    /// Resolves when the leader ends.
    pub(super) exit: Exit,
    /// Output as it is read, closed once every reader reaches its end.
    pub(super) output: mpsc::Receiver<Chunk>,
}

/// Starts processes one way.
pub(super) trait Launcher: Send + Sync {
    /// Starts `plan` as the leader of a new process group.
    fn launch(&self, plan: &Plan) -> io::Result<Spawned>;
}

/// A process's input.
#[async_trait::async_trait]
pub(super) trait Input: Send {
    /// Writes and flushes `data`.
    async fn write(&mut self, data: Bytes) -> io::Result<()>;
}

mod terminal;

use self::terminal::Terminal;

/// The launcher for `terminal`.
pub(super) fn launcher(terminal: bool) -> &'static dyn Launcher {
    if terminal { &Terminal } else { &Pipes }
}

/// Standard input, output and error as three pipes.
struct Pipes;

impl Launcher for Pipes {
    fn launch(&self, plan: &Plan) -> io::Result<Spawned> {
        let mut child = tokio::process::Command::new(&plan.program)
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
            .kill_on_drop(true)
            .spawn()?;
        let pid = leader(child.id())?;
        let (sender, output) = mpsc::channel(OUTPUT_BACKLOG);
        let (stdin, stdout, stderr) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (stdin, stdout, stderr) = stdin
            .zip(stdout)
            .zip(stderr)
            .map(|((i, o), e)| (i, o, e))
            .ok_or_else(missing_pipe)?;
        tokio::spawn(pump(stdout, Stream::Stdout, sender.clone()));
        tokio::spawn(pump(stderr, Stream::Stderr, sender));
        let exit = Box::pin(async move { child.wait().await.map_or(Status::Lost, Status::from) });
        Ok(Spawned {
            pid,
            input: Box::new(PipeInput(stdin)),
            exit,
            output,
        })
    }
}

/// Forwards one pipe's output until it closes or its reader is gone.
async fn pump(reader: impl AsyncRead + Unpin, stream: Stream, sender: mpsc::Sender<Chunk>) {
    let mut chunks = ReaderStream::new(reader);
    while let Some(Ok(data)) = chunks.next().await {
        if sender.send(Chunk { stream, data }).await.is_err() {
            break;
        }
    }
}

/// A pipe process's standard input.
struct PipeInput(ChildStdin);

#[async_trait::async_trait]
impl Input for PipeInput {
    async fn write(&mut self, data: Bytes) -> io::Result<()> {
        self.0.write_all(&data).await?;
        self.0.flush().await
    }
}

/// The group a just-started leader leads.
fn leader(id: Option<u32>) -> io::Result<Pid> {
    id.and_then(|raw| i32::try_from(raw).ok())
        .and_then(Pid::from_raw)
        .ok_or_else(missing_pipe)
}

/// A handle a started process should have and does not.
fn missing_pipe() -> io::Error {
    io::Error::from(io::ErrorKind::BrokenPipe)
}

#[cfg(test)]
#[path = "launch/tests.rs"]
mod tests;
