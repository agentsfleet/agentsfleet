//! The environment of a standalone harness: this host's processes and the
//! files under one directory.
//!
//! A process runs as the user that runs the harness, with that user's
//! environment and reach, so `LocalEnv` confines nothing a command chooses to
//! do: it is for a harness on a developer's machine. An embedder whose host
//! holds what a command must not read, such as a provider key, gives its
//! tools a sandbox instead and never this.
//!
//! Every process runs on pipes, a terminal asked for included. A path is
//! held under the root by name and, once resolved, by where its deepest
//! existing part really is, so a link out of the root is refused.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

use super::{
    DirEntry, EntryKind, Env, EnvError, EnvErrorKind, EnvFuture, FileContent, Listing, Process,
    ProcessId, Spawn, Stream, pipe,
};
use crate::tools::env::{Ending, Feed};

/// How long a process's pipes may stay open after it exits, held by
/// something it started, before its output is given up.
const DRAIN_GRACE: Duration = Duration::from_secs(2);
/// The most entries one listing carries.
const LIST_MAX: usize = 1000;
/// How much one read of a pipe takes.
const READ_CHUNK: usize = 16 * 1024;
/// The signal a kill ends a process with.
const KILLED: i32 = 9;
/// What a path that leaves the root reads back.
const LEAVES_ROOT: &str = "the path leaves the workspace";
/// What a call naming a process the environment no longer holds reads back.
const UNKNOWN_PROCESS: &str = "no such process";
/// What a write to a process that closed its input reads back.
const INPUT_CLOSED: &str = "the process closed its input";
/// What a spawn with no program reads back.
const NO_PROGRAM: &str = "a spawn names no program";

/// One running process, as the environment reaches it.
#[derive(Debug)]
struct Running {
    input: mpsc::UnboundedSender<Bytes>,
    kill: Arc<Notify>,
}

/// This host's processes and the files under one directory.
#[derive(Debug)]
pub struct LocalEnv {
    root: PathBuf,
    shown: String,
    running: Arc<Mutex<BTreeMap<ProcessId, Running>>>,
    next: AtomicU64,
}

impl LocalEnv {
    /// An environment rooted at `root`, which must exist; the model knows the
    /// root by its absolute path.
    ///
    /// # Errors
    /// `root` does not exist or cannot be resolved.
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = std::fs::canonicalize(root)?;
        let shown = root.display().to_string();
        Ok(Self {
            root,
            shown,
            running: Arc::default(),
            next: AtomicU64::new(1),
        })
    }

    /// The processes still running.
    fn running(&self) -> MutexGuard<'_, BTreeMap<ProcessId, Running>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `path` under the root, or the refusal: no `..`, no absolute path
    /// outside the root, and no link out of it along the way.
    async fn resolve(&self, path: &str) -> Result<PathBuf, EnvError> {
        let given = Path::new(path);
        let relative = given.strip_prefix(&self.root).unwrap_or(given);
        let climbs = relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
        if climbs {
            return Err(EnvError::new(EnvErrorKind::PathRefused, LEAVES_ROOT));
        }
        let full = self.root.join(relative);
        let mut probe = Some(full.as_path());
        while let Some(part) = probe {
            if let Ok(real) = tokio::fs::canonicalize(part).await {
                if !real.starts_with(&self.root) {
                    return Err(EnvError::new(EnvErrorKind::PathRefused, LEAVES_ROOT));
                }
                break;
            }
            probe = part.parent();
        }
        Ok(full)
    }

    async fn start(&self, spawn: &Spawn) -> Result<Process, EnvError> {
        let (program, args) = spawn
            .argv()
            .split_first()
            .ok_or_else(|| EnvError::new(EnvErrorKind::Unavailable, NO_PROGRAM))?;
        let cwd = match spawn.working_directory() {
            Some(cwd) => self.resolve(cwd).await?,
            None => self.root.clone(),
        };
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .envs(spawn.environment())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|failure| failed(&failure))?;
        let id = ProcessId::new(self.next.fetch_add(1, Ordering::Relaxed));
        let (input, queued) = mpsc::unbounded_channel();
        let kill = Arc::new(Notify::new());
        if let Some(stdin) = child.stdin.take() {
            tokio::spawn(feed_input(stdin, queued));
        }
        let (output, said) = mpsc::unbounded_channel();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(read_pipe(stdout, Stream::Stdout, output.clone()));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(read_pipe(stderr, Stream::Stderr, output));
        }
        let (feed, events) = pipe();
        self.running().insert(
            id,
            Running {
                input,
                kill: Arc::clone(&kill),
            },
        );
        let watch = Watch {
            child,
            said,
            feed,
            kill,
            deadline: spawn.time_limit().map(|limit| Instant::now() + limit),
        };
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            watch.run().await;
            running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
        });
        Ok(Process {
            id,
            events: Box::new(events),
        })
    }
}

impl Env for LocalEnv {
    fn root(&self) -> &str {
        &self.shown
    }

    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process> {
        Box::pin(self.start(spawn))
    }

    fn write(&self, process: ProcessId, data: Bytes) -> EnvFuture<'_, ()> {
        let sent = self.running().get(&process).map_or_else(
            || Err(EnvError::new(EnvErrorKind::UnknownProcess, UNKNOWN_PROCESS)),
            |running| {
                running
                    .input
                    .send(data)
                    .map_err(|_closed| EnvError::new(EnvErrorKind::InputRefused, INPUT_CLOSED))
            },
        );
        Box::pin(std::future::ready(sent))
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        let killed = self.running().get(&process).map_or_else(
            || Err(EnvError::new(EnvErrorKind::UnknownProcess, UNKNOWN_PROCESS)),
            |running| {
                running.kill.notify_one();
                Ok(())
            },
        );
        Box::pin(std::future::ready(killed))
    }

    fn read_file<'a>(&'a self, path: &'a str, max_bytes: u64) -> EnvFuture<'a, FileContent> {
        Box::pin(async move {
            let full = self.resolve(path).await?;
            let file = tokio::fs::File::open(&full)
                .await
                .map_err(|failure| failed(&failure))?;
            let length = file
                .metadata()
                .await
                .map_err(|failure| failed(&failure))?
                .len();
            let mut data = Vec::new();
            file.take(max_bytes)
                .read_to_end(&mut data)
                .await
                .map_err(|failure| failed(&failure))?;
            Ok(FileContent {
                data: Bytes::from(data),
                truncated: length > max_bytes,
            })
        })
    }

    fn write_file<'a>(&'a self, path: &'a str, data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(async move {
            let full = self.resolve(path).await?;
            if let Some(parent) = full.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|failure| failed(&failure))?;
            }
            tokio::fs::write(&full, &data)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn append_file<'a>(&'a self, path: &'a str, data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(async move {
            let full = self.resolve(path).await?;
            if let Some(parent) = full.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|failure| failed(&failure))?;
            }
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&full)
                .await
                .map_err(|failure| failed(&failure))?;
            file.write_all(&data)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn delete_file<'a>(&'a self, path: &'a str) -> EnvFuture<'a, ()> {
        Box::pin(async move {
            let full = self.resolve(path).await?;
            let metadata = tokio::fs::symlink_metadata(&full)
                .await
                .map_err(|failure| failed(&failure))?;
            if !metadata.is_file() {
                return Err(EnvError::new(
                    EnvErrorKind::Unavailable,
                    format!("{path} is not a regular file"),
                ));
            }
            tokio::fs::remove_file(&full)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn list_dir<'a>(&'a self, path: &'a str) -> EnvFuture<'a, Listing> {
        Box::pin(async move {
            let full = self.resolve(path).await?;
            let mut read = tokio::fs::read_dir(&full)
                .await
                .map_err(|failure| failed(&failure))?;
            let mut listing = Listing::default();
            while let Some(entry) = read
                .next_entry()
                .await
                .map_err(|failure| failed(&failure))?
            {
                if listing.entries.len() == LIST_MAX {
                    listing.truncated = true;
                    break;
                }
                let metadata = tokio::fs::symlink_metadata(entry.path())
                    .await
                    .map_err(|failure| failed(&failure))?;
                let kind = if metadata.is_symlink() {
                    EntryKind::Symlink
                } else if metadata.is_dir() {
                    EntryKind::Directory
                } else if metadata.is_file() {
                    EntryKind::File
                } else {
                    EntryKind::Other
                };
                listing.entries.push(DirEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    kind,
                    size: metadata.len(),
                });
            }
            Ok(listing)
        })
    }
}

/// An operating-system failure, sorted: a name that is not there, or
/// anything else in the system's own words.
fn failed(failure: &io::Error) -> EnvError {
    let kind = if failure.kind() == io::ErrorKind::NotFound {
        EnvErrorKind::NotFound
    } else {
        EnvErrorKind::Unavailable
    };
    EnvError::new(kind, failure.to_string())
}

/// Writes what the calls queue to the process's input, until it closes it.
async fn feed_input(mut stdin: ChildStdin, mut queued: mpsc::UnboundedReceiver<Bytes>) {
    while let Some(data) = queued.recv().await {
        if stdin.write_all(&data).await.is_err() {
            break;
        }
    }
}

/// Hands each chunk `pipe` says to the watcher, until it closes.
async fn read_pipe(
    mut pipe: impl AsyncRead + Unpin,
    stream: Stream,
    said: mpsc::UnboundedSender<(Stream, Bytes)>,
) {
    let mut buffer = vec![0; READ_CHUNK];
    while let Ok(read) = pipe.read(&mut buffer).await {
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            break;
        };
        if said.send((stream, Bytes::copy_from_slice(chunk))).is_err() {
            break;
        }
    }
}

/// One process from its start to its end: its output fed on, a kill or its
/// timeout carried out, and its ending said last.
struct Watch {
    child: Child,
    said: mpsc::UnboundedReceiver<(Stream, Bytes)>,
    feed: Feed,
    kill: Arc<Notify>,
    deadline: Option<Instant>,
}

impl Watch {
    async fn run(mut self) {
        let mut cause: Option<Ending> = None;
        let status = loop {
            let deadline = self.deadline;
            tokio::select! {
                status = self.child.wait() => break status.ok(),
                () = self.kill.notified() => {
                    let _signalled = self.child.start_kill();
                    cause.get_or_insert(Ending::Signaled(KILLED));
                }
                () = sleep_until(deadline) => {
                    let _signalled = self.child.start_kill();
                    cause.get_or_insert(Ending::TimedOut);
                    self.deadline = None;
                }
                Some((stream, data)) = self.said.recv() => self.feed.output(stream, data),
            }
        };
        let drained = Instant::now() + DRAIN_GRACE;
        let mut abandoned = false;
        loop {
            tokio::select! {
                next = self.said.recv() => match next {
                    Some((stream, data)) => self.feed.output(stream, data),
                    None => break,
                },
                () = tokio::time::sleep_until(drained) => {
                    abandoned = true;
                    break;
                }
            }
        }
        let ending = cause.unwrap_or_else(|| status.map_or(Ending::Interrupted, ending_of));
        self.feed.end(ending, abandoned);
    }
}

/// Sleeps until `deadline`, or forever when there is none.
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// How a process that exited with `status` ended.
fn ending_of(status: ExitStatus) -> Ending {
    if let Some(code) = status.code() {
        return Ending::Exited(code);
    }
    signal_of(status).map_or(Ending::Interrupted, Ending::Signaled)
}

#[cfg(unix)]
fn signal_of(status: ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(&status)
}

#[cfg(not(unix))]
fn signal_of(_status: ExitStatus) -> Option<i32> {
    None
}
