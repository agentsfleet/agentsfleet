//! Where the generic tools run: the seam between a tool and the machine its
//! commands and files live on.
//!
//! ```text
//!  shell ─┐                         ┌─► LocalEnv: this host's processes and files
//!  exec_command, write_stdin ─┼─► Env ─┤
//!  file_*, apply_patch, ls ──┘         └─► an embedder's sandbox, such as a
//!                                          microVM's executor over a socket
//! ```
//!
//! A tool never starts a process or opens a file itself: it asks its
//! [`Env`], so one tool serves a laptop and a sandbox alike. The shape is the
//! executor's a sandboxing embedder already has: start a process and read its
//! events, write to it, kill it, and read, write, append, delete and list
//! files under one root. Every path is relative to that root, and one that
//! leaves it, by name or through a link, is the environment's to refuse.

mod error;
mod local;
mod pipe;

use std::collections::BTreeMap;
use std::fmt;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use serde_json::Value;

use crate::tool::Image;

pub use self::error::{EnvError, EnvErrorKind};
pub use self::local::LocalEnv;
pub use self::pipe::{Feed, Piped, pipe};

/// The most of one file a read carries: a file longer than this is read to
/// here and marked truncated.
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;
/// The most of a process's unread output an environment keeps at each edge,
/// its head and its tail; what falls between them is dropped and counted.
pub const EDGE_BYTES: usize = 512 * 1024;
/// How long past a command's timeout an environment takes to report how it
/// ended, when it says nothing else: a kill's grace, a drain's grace, and a
/// second to deliver the answer.
pub const ENDING_GRACE_DEFAULT: Duration = Duration::from_secs(5);
/// What a browser tool on an environment without a browser reads back after
/// the tool's name.
const NO_BROWSER: &str = "has no browser in this environment";

/// One environment call's work.
pub type EnvFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, EnvError>> + Send + 'a>>;

/// A process the environment started, unique for the life of one
/// environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessId(u64);

impl ProcessId {
    /// Wraps the number the environment assigned.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The number the environment assigned.
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
    /// This signal ended it.
    Signaled(i32),
    /// Its timeout elapsed and the environment killed it.
    TimedOut,
    /// It was killed because its environment ran out of memory.
    OutOfMemory,
    /// No status reached the caller: the environment went away, or could
    /// not learn how the process ended.
    Interrupted,
}

/// What a POSIX shell adds to a signal's number to report a child that
/// signal killed as its own exit status.
const SHELL_SIGNAL_BASE: i32 = 128;
/// The signal an out-of-memory kill sends.
const SIGKILL: i32 = 9;

impl Ending {
    /// How it ended, as a log line names it.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Exited(_) => "exited",
            Self::Signaled(_) => "signaled",
            Self::TimedOut => "timed_out",
            Self::OutOfMemory => "out_of_memory",
            Self::Interrupted => "interrupted",
        }
    }

    /// The exit status a shell reports for this ending: the process's own,
    /// or the shell's base plus the signal that ended it, saturating; an
    /// out-of-memory kill reads as the `SIGKILL` it was. `None` when no
    /// status arrived.
    #[must_use]
    pub const fn exit_code(self) -> Option<i32> {
        match self {
            Self::Exited(code) => Some(code),
            Self::Signaled(signal) => Some(SHELL_SIGNAL_BASE.saturating_add(signal)),
            Self::OutOfMemory => Some(SHELL_SIGNAL_BASE + SIGKILL),
            Self::TimedOut | Self::Interrupted => None,
        }
    }
}

/// What the environment says about one process after it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEvent {
    /// A chunk of output.
    Output {
        /// Where it came from.
        stream: Stream,
        /// The bytes, unmodified; output need not be text.
        data: Bytes,
    },
    /// Output that fell between the head and the tail of what its reader had
    /// not read, dropped unread.
    Omitted {
        /// How many bytes.
        bytes: u64,
    },
    /// The last event a process produces.
    Ended {
        /// How it ended.
        ending: Ending,
        /// Whether output still arriving when it ended was left behind.
        output_abandoned: bool,
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

    /// Runs it in `cwd`, relative to the root, rather than the root itself.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Sets one environment variable.
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

    /// Kills it once `timeout` elapses.
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

    /// The variables it starts with, beside what its environment gives it.
    #[must_use]
    pub const fn environment(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Whether it runs on a pseudo-terminal.
    #[must_use]
    pub const fn on_terminal(&self) -> bool {
        self.terminal
    }

    /// When the environment kills it, if ever.
    #[must_use]
    pub const fn time_limit(&self) -> Option<Duration> {
        self.timeout
    }
}

/// The reading end of one process's events: its output in order, a count
/// where output was dropped unread, then exactly one
/// [`ProcessEvent::Ended`].
///
/// A trait so each environment keeps its own channel: a socket's client
/// feeds one bounded store, a host process another, and a tool reads both
/// alike. What a reader has not read is bounded to a head and a tail.
pub trait Events: Send + fmt::Debug {
    /// The next event, waiting for one; `None` once the ending was read, or
    /// the process went away without one.
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<ProcessEvent>> + Send + '_>>;

    /// Every event already waiting, in order, and whether nothing more can
    /// arrive. Later arrivals stay for the next read.
    fn take_ready(&mut self) -> (Vec<ProcessEvent>, bool);

    /// Whether anything more can arrive: once the process ended, or its
    /// environment did, what is left is only to be read.
    fn is_finished(&self) -> bool;

    /// Waits, reading nothing, until nothing more can arrive.
    fn finished(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    /// Gives up on the process's ending: a reader that stopped waiting for
    /// one says so, and the environment frees what it held for it.
    fn disconnect(&mut self);
}

/// A started process: its identifier and the events it says.
#[derive(Debug)]
pub struct Process {
    /// What to name it in [`Env::write`] and [`Env::kill`].
    pub id: ProcessId,
    /// Output, then exactly one [`ProcessEvent::Ended`].
    pub events: Box<dyn Events>,
}

impl Process {
    /// Reads the process to its end, handing each chunk of output to
    /// `output`; `None` when the events finished before an ending arrived.
    pub async fn ended(mut self, mut output: impl FnMut(Stream, Bytes)) -> Option<Ending> {
        while let Some(event) = self.events.recv().await {
            match event {
                ProcessEvent::Output { stream, data } => output(stream, data),
                ProcessEvent::Omitted { .. } => {}
                ProcessEvent::Ended { ending, .. } => return Some(ending),
            }
        }
        None
    }
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

/// A directory's entries, up to the most one answer carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

/// A browser an environment drives for the browser tools.
pub trait Browser: Send + Sync + fmt::Debug {
    /// Opens `url` and answers what the page says, as the model reads it.
    fn open<'a>(&'a self, arguments: &'a Value) -> EnvFuture<'a, String>;

    /// Acts on the open page: click, type, scroll, read.
    fn act<'a>(&'a self, arguments: &'a Value) -> EnvFuture<'a, String>;

    /// A screenshot of the open page.
    fn screenshot<'a>(&'a self, arguments: &'a Value) -> EnvFuture<'a, Image>;
}

/// Processes and files under one root, wherever they run.
///
/// Every path is relative to [`Env::root`], or absolute under it; one that
/// leaves it, through `..` or a link, is refused
/// ([`EnvErrorKind::PathRefused`]). Every method answers a boxed future so
/// a tool holds any environment as `&dyn Env`.
pub trait Env: Send + Sync + fmt::Debug {
    /// The absolute path the model knows the root by, such as `/work`:
    /// a path the model writes under it is read relative to the root.
    fn root(&self) -> &str;

    /// Starts a process and returns the events it says.
    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process>;

    /// Queues bytes for a running process's input, answered once they are
    /// queued, not once the process has read them.
    fn write(&self, process: ProcessId, data: Bytes) -> EnvFuture<'_, ()>;

    /// Ends a process and everything it started.
    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()>;

    /// Reads a regular file, at most `max_bytes` of it.
    fn read_file<'a>(&'a self, path: &'a str, max_bytes: u64) -> EnvFuture<'a, FileContent>;

    /// Writes a regular file, replacing what was there and making any
    /// missing directory on the way.
    fn write_file<'a>(&'a self, path: &'a str, data: Bytes) -> EnvFuture<'a, ()>;

    /// Adds to the end of a regular file, making it when absent.
    fn append_file<'a>(&'a self, path: &'a str, data: Bytes) -> EnvFuture<'a, ()>;

    /// Removes a regular file; a link, a directory or a device is refused.
    fn delete_file<'a>(&'a self, path: &'a str) -> EnvFuture<'a, ()>;

    /// Lists a directory, up to the most one answer carries.
    fn list_dir<'a>(&'a self, path: &'a str) -> EnvFuture<'a, Listing>;

    /// How long past a command's timeout this environment takes to report
    /// how it ended; a command's wait gives up after its timeout and this.
    fn ending_grace(&self) -> Duration {
        ENDING_GRACE_DEFAULT
    }

    /// The browser the browser tools drive.
    ///
    /// # Errors
    /// The environment has none: why, as the end of a sentence that starts
    /// with the tool's name.
    fn browser(&self) -> Result<&dyn Browser, &str> {
        Err(NO_BROWSER)
    }
}
