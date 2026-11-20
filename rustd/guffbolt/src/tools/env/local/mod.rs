//! The environment of a standalone harness: this host's processes and the
//! files under one directory.
//!
//! A process runs as the user that runs the harness, with that user's reach,
//! so `LocalEnv` confines nothing a command chooses to do: it is for a
//! harness on a developer's machine. An embedder whose host holds what a
//! command must not read, such as a provider key, gives its tools a sandbox
//! instead and never this.
//!
//! A process starts with only the variables its spawn names and a search
//! path, never the harness's own environment, and leads a process group of
//! its own, which a stop or a timeout kills whole ([`process`]). It runs on
//! pipes, or on a pseudo-terminal when its spawn asks for one on a Unix host
//! ([`terminal`]). A path is held under the root by name and, once resolved,
//! by where its deepest existing part really is, so a link out of the root
//! is refused.

mod process;
#[cfg(unix)]
mod terminal;

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

use self::process::Watch;
use super::{
    DirEntry, EntryKind, Env, EnvError, EnvErrorKind, EnvFuture, FileContent, Listing, Process,
    ProcessId, Spawn, pipe,
};

/// The most entries one listing carries.
const LIST_MAX: usize = 1000;
/// What a path that leaves the root reads back.
const LEAVES_ROOT: &str = "the path leaves the working directory";
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
        let command = process::command(program, args, &cwd, spawn);
        let (input, queued) = mpsc::unbounded_channel();
        let (output, said) = mpsc::unbounded_channel();
        #[cfg(unix)]
        let started = if spawn.on_terminal() {
            terminal::on_terminal(command, output, queued)
        } else {
            process::on_pipes(command, output, queued)
        };
        // A host with no pseudo-terminal runs a terminal asked for on pipes.
        #[cfg(not(unix))]
        let started = process::on_pipes(command, output, queued);
        let child = started.map_err(|failure| failed(&failure))?;
        let id = ProcessId::new(self.next.fetch_add(1, Ordering::Relaxed));
        let kill = Arc::new(Notify::new());
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
            // tokio's file hands a write to its blocking pool, and `write_all`
            // can answer before the bytes land; the flush waits for them, so a
            // read after the call sees what it appended.
            let appended = async {
                file.write_all(&data).await?;
                file.flush().await
            };
            appended.await.map_err(|failure| failed(&failure))
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

#[cfg(all(test, unix))]
mod tests;
