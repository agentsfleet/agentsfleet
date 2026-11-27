//! A tree on the host read as the changes a save publishes: every entry in
//! path order, each file's chunks right after it.
//!
//! The walk runs on a blocking thread and hands one change at a time across a
//! one-slot channel, so a reader holds at most a chunk or two of plaintext. A
//! reader that stops early ends the walk; a walk that fails or panics ends the
//! stream with its error, never with a silent short read.

use std::path::PathBuf;

use af_chunk_store::{Change, ChunkKey};
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointEntry, CheckpointFile};
use futures_util::{StreamExt as _, stream::BoxStream};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use super::files::{self, Entry, Workspace};
use super::names::{EveryNameItsOwnFile, Naming, SecondNames};
use crate::{Result, error};

/// Where a save reads its changes from; read as often as the save needs.
pub(crate) trait ChangeSource: Send + Sync {
    /// Every change, from the first.
    fn changes(&self) -> BoxStream<'_, Result<Change>>;

    /// The generation the changes are the difference from; none when they
    /// are the whole tree.
    fn parent(&self) -> Option<&Uuid7> {
        None
    }
}

/// A tree already on the host, read whole each time.
#[derive(Debug, Clone)]
pub(crate) struct Tree {
    root: PathBuf,
    stop: CancellationToken,
}

impl Tree {
    /// The tree at `root`, whose walk ends when `stop` is cancelled.
    pub(crate) const fn new(root: PathBuf, stop: CancellationToken) -> Self {
        Self { root, stop }
    }
}

impl ChangeSource for Tree {
    fn changes(&self) -> BoxStream<'_, Result<Change>> {
        let (root, stop) = (self.root.clone(), self.stop.clone());
        let (send, receive) = mpsc::channel(1);
        let walk = tokio::task::spawn_blocking(move || {
            let workspace = Workspace::open(&root)?;
            walk(&workspace, &stop, &mut EveryNameItsOwnFile, &mut |change| {
                Ok(send.blocking_send(change).is_ok())
            })
        });
        futures_util::stream::unfold(Some((receive, walk)), next).boxed()
    }
}

/// The walk's next change; once the walk is done, its failure, if it had one.
async fn next(
    state: Option<(mpsc::Receiver<Change>, JoinHandle<Result<()>>)>,
) -> Option<(
    Result<Change>,
    Option<(mpsc::Receiver<Change>, JoinHandle<Result<()>>)>,
)> {
    let (mut receive, walk) = state?;
    if let Some(change) = receive.recv().await {
        return Some((Ok(change), Some((receive, walk))));
    }
    match walk.await {
        Ok(Ok(())) => None,
        Ok(Err(failure)) => Some((Err(failure), None)),
        Err(panicked) => Some((Err(panicked.into()), None)),
    }
}

/// Hands `emit` every entry of `workspace` in path order, each file's chunks
/// after its entry, asking `names` once per file how its name is saved. Stops
/// early, with no error, once `emit` answers that nothing reads on.
///
/// # Errors
/// The inventory refused the tree, a file changed while it was read, or the
/// walk was cancelled.
pub(super) fn walk(
    workspace: &Workspace,
    stop: &CancellationToken,
    names: &mut dyn SecondNames,
    emit: &mut dyn FnMut(Change) -> Result<bool>,
) -> Result<()> {
    let inventory = workspace.inventory(stop)?;
    for (path, entry) in &inventory {
        if stop.is_cancelled() {
            return Err(error::config("checkpoint inventory cancelled"));
        }
        let listed = match entry {
            Entry::Directory => CheckpointEntry::Directory { path: path.clone() },
            Entry::Symlink { target } => CheckpointEntry::Symlink {
                path: path.clone(),
                target: target.clone(),
            },
            Entry::File {
                identity,
                executable,
            } => {
                let Naming::OwnFile = names.naming(path, identity);
                CheckpointEntry::File(CheckpointFile {
                    path: path.clone(),
                    executable: *executable,
                    size: files::size(entry),
                    objects: Vec::new(),
                })
            }
        };
        if !emit(Change::Entry(listed))? {
            return Ok(());
        }
        if !chunks(workspace, stop, path, entry, emit)? {
            return Ok(());
        }
    }
    Ok(())
}

/// Hands `emit` each chunk of the file `entry` at `path`; whether a reader
/// still reads on.
fn chunks(
    workspace: &Workspace,
    stop: &CancellationToken,
    path: &str,
    entry: &Entry,
    emit: &mut dyn FnMut(Change) -> Result<bool>,
) -> Result<bool> {
    let Entry::File { identity, .. } = entry else {
        return Ok(true);
    };
    for (index, offset) in (0..files::size(entry)).step_by(chunk_step()?).enumerate() {
        if stop.is_cancelled() {
            return Err(error::config("checkpoint hashing cancelled"));
        }
        let bytes = workspace.chunk(path, identity, offset)?;
        let key = ChunkKey::File {
            path: path.to_owned(),
            index: crate::byte_count(index),
        };
        if !emit(Change::Chunk { key, bytes })? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// One chunk's span, as the step a walk over a file's offsets takes.
fn chunk_step() -> Result<usize> {
    usize::try_from(files::CHUNK)
        .map_err(|source| error::boundary("checkpoint chunk size overflow", source))
}
