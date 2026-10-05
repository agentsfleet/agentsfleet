//! What the supervisor asks of a sandbox, as the trait the agent loop calls.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    /// The process's standard output, when it runs on pipes.
    Stdout,
    /// The process's standard error, when it runs on pipes.
    Stderr,
    /// The pseudo-terminal, which merges both.
    Terminal,
}

/// How a process ended: exactly one of these, exactly once.
///
/// The executor's `process/exited` carries this as it is, so the wire and the
/// caller spell an ending one way: `{"kind":"exited","code":0}`,
/// `{"kind":"signaled","code":9}`, `{"kind":"timed_out"}`,
/// `{"kind":"interrupted"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "code", rename_all = "snake_case")]
pub enum Ending {
    /// It exited with this status.
    Exited(i32),
    /// This signal ended it.
    Signaled(i32),
    /// Its timeout elapsed and the executor killed its group.
    TimedOut,
    /// No status reached the caller: the executor or its sandbox went away,
    /// or the executor could not learn how the process ended.
    Interrupted,
}

impl Ending {
    /// How it ended, spelled as the wire's `kind`, for a log line to carry as
    /// a field; a test holds the two spellings together.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Exited(_) => "exited",
            Self::Signaled(_) => "signaled",
            Self::TimedOut => "timed_out",
            Self::Interrupted => "interrupted",
        }
    }

    /// The exit status or the signal number, when the ending carries one.
    #[must_use]
    pub const fn code(self) -> Option<i32> {
        match self {
            Self::Exited(code) | Self::Signaled(code) => Some(code),
            Self::TimedOut | Self::Interrupted => None,
        }
    }
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
///
/// The channel is unbounded so that one caller slow to drain it never stalls
/// the connection every other process's events share. What it can hold is
/// bounded where the output is produced: the executor forwards at most a head
/// and a tail of each process's output.
#[derive(Debug)]
pub struct Process {
    /// What to name it in [`Executor::write`] and [`Executor::kill`].
    pub id: ProcessId,
    /// Output, then exactly one [`ProcessEvent::Ended`].
    pub events: mpsc::UnboundedReceiver<ProcessEvent>,
}

impl Process {
    /// Reads the process to its end, handing each chunk of output to
    /// `output`; `None` when the channel closed before an ending arrived.
    pub async fn ended(mut self, mut output: impl FnMut(Stream, Bytes)) -> Option<Ending> {
        while let Some(event) = self.events.recv().await {
            match event {
                ProcessEvent::Output { stream, data } => output(stream, data),
                ProcessEvent::Ended { ending, .. } => return Some(ending),
            }
        }
        None
    }
}

/// What a directory entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    /// The entry's name, without its directory.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// Its size in bytes, as the file system reports it.
    pub size: u64,
}

/// A directory's entries, up to the most one answer carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// The entries, in no particular order.
    pub entries: Vec<DirEntry>,
    /// Whether the directory held more than were listed.
    pub truncated: bool,
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
    async fn spawn(&self, spawn: &Spawn) -> Result<Process>;

    /// Queues bytes for a running process's input.
    ///
    /// Answered once the bytes are queued, not once the process has read
    /// them, so a process that never reads cannot stall its caller; a write
    /// past the queue's bound is refused instead.
    async fn write(&self, process: ProcessId, data: Bytes) -> Result<()>;

    /// Ends a process and every descendant in its group.
    async fn kill(&self, process: ProcessId) -> Result<()>;

    /// Reads a regular file, at most `max_bytes` of it.
    async fn read_file(&self, path: &str, max_bytes: u64) -> Result<FileContent>;

    /// Writes a regular file, replacing what was there.
    async fn write_file(&self, path: &str, data: Bytes) -> Result<()>;

    /// Adds to the end of a regular file, making it when absent.
    async fn append_file(&self, path: &str, data: Bytes) -> Result<()>;

    /// Removes a regular file; a link, a directory or a device is refused.
    async fn delete_file(&self, path: &str) -> Result<()>;

    /// Lists a directory, up to the most one answer carries.
    async fn list_dir(&self, path: &str) -> Result<Listing>;
}
