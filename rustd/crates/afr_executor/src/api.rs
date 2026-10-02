//! What the supervisor asks of a sandbox, as the trait the agent loop calls.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use crate::error::Result;

/// A process the executor started, unique for the life of one executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessId(u64);

impl ProcessId {
    /// Wraps the number the executor assigned.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The number the executor assigned.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Which stream a chunk of output came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// The process's standard output, when it runs on pipes.
    Stdout,
    /// The process's standard error, when it runs on pipes.
    Stderr,
    /// The pseudo-terminal, which merges both.
    Terminal,
}

/// How a process ended: exactly one of these, exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// It exited with this status.
    Exited(i32),
    /// A signal ended it.
    Signaled(i32),
    /// Its timeout elapsed and the executor killed its group.
    TimedOut,
    /// The executor or its sandbox went away before the process reported an
    /// end, so the supervisor closed it.
    Interrupted,
}

/// What the executor says about one process after it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEvent {
    /// A chunk of output.
    Output {
        /// Where it came from.
        stream: Stream,
        /// The bytes, unmodified; output need not be text.
        data: Bytes,
    },
    /// The last event a process produces.
    Ended {
        /// How it ended.
        ending: Ending,
        /// Output bytes dropped between the kept head and tail.
        omitted_bytes: u64,
    },
}

/// A process to start: its program and arguments, and how to run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawn {
    argv: Vec<String>,
    cwd: Option<String>,
    env: BTreeMap<String, String>,
    terminal: bool,
    timeout: Option<Duration>,
}

impl Spawn {
    /// Starts a request for `program`, with no arguments yet.
    #[must_use]
    pub fn program(program: impl Into<String>) -> Self {
        Self {
            argv: vec![program.into()],
            cwd: None,
            env: BTreeMap::new(),
            terminal: false,
            timeout: None,
        }
    }

    /// Appends one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.argv.push(arg.into());
        self
    }

    /// Appends every argument in order.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.argv.extend(args.into_iter().map(Into::into));
        self
    }

    /// Runs it in `cwd` rather than the workspace root.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Sets one environment variable; the sandbox starts from an empty one.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Runs it on a pseudo-terminal rather than pipes.
    #[must_use]
    pub const fn terminal(mut self) -> Self {
        self.terminal = true;
        self
    }

    /// Kills its group once `timeout` elapses.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// The program followed by its arguments; never empty.
    #[must_use]
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// The working directory, when one was set.
    #[must_use]
    pub fn working_directory(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// The environment it starts with.
    #[must_use]
    pub const fn environment(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Whether it runs on a pseudo-terminal.
    #[must_use]
    pub const fn on_terminal(&self) -> bool {
        self.terminal
    }

    /// When the executor kills it, if ever.
    #[must_use]
    pub const fn time_limit(&self) -> Option<Duration> {
        self.timeout
    }
}

/// A started process: its identifier and the channel its events arrive on.
#[derive(Debug)]
pub struct Process {
    /// What to name it in [`Executor::write`] and [`Executor::kill`].
    pub id: ProcessId,
    /// Output, then exactly one [`ProcessEvent::Ended`].
    pub events: mpsc::Receiver<ProcessEvent>,
}

/// What a directory entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link, not followed.
    Symlink,
    /// Anything else: a socket, a device, a pipe.
    Other,
}

/// One entry of a listed directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The entry's name, without its directory.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// Its size in bytes, as the file system reports it.
    pub size: u64,
}

/// A file's bytes, up to the limit the caller asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContent {
    /// The bytes read.
    pub data: Bytes,
    /// Whether the file held more than was read.
    pub truncated: bool,
}

/// Processes and files inside one sandbox.
///
/// Every path is inside `/workspace`; one that escapes it, through `..` or a
/// symbolic link, is refused.
#[async_trait::async_trait]
pub trait Executor: Send + Sync + fmt::Debug {
    /// Starts a process and returns the channel its events arrive on.
    async fn spawn(&self, spawn: Spawn) -> Result<Process>;

    /// Writes to a running process's input.
    async fn write(&self, process: ProcessId, data: Bytes) -> Result<()>;

    /// Ends a process and every descendant in its group.
    async fn kill(&self, process: ProcessId) -> Result<()>;

    /// Reads a file, at most `max_bytes` of it.
    async fn read_file(&self, path: &str, max_bytes: u64) -> Result<FileContent>;

    /// Writes a file, replacing what was there.
    async fn write_file(&self, path: &str, data: Bytes) -> Result<()>;

    /// Lists a directory.
    async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>>;
}
