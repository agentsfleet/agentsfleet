//! Fetch bounded groups across file boundaries and keep a single ordered writer.

use super::files::{Restoring, Workspace};
use crate::{Result, error};
use af_crypto::secret::SecretBytes;
use af_wire::task::CheckpointEntry;
use futures_util::future::BoxFuture;
use std::{collections::BTreeMap, iter::Peekable, path::PathBuf, sync::Arc};

pub(super) type Chunk = Arc<SecretBytes>;

// One file chunk is at most eight MiB. The count also bounds the native request.
const CHUNKS: usize = 4;
const ENTRIES: usize = 32;

struct FileStart {
    path: String,
    size: u64,
    executable: bool,
}

enum Step {
    Open(FileStart),
    Chunk(String),
    Finish,
    Link { path: String, target: String },
    Remove,
}

struct EntrySteps {
    leading: Option<Step>,
    chunks: std::vec::IntoIter<String>,
    trailing: Option<Step>,
}

impl EntrySteps {
    fn from_entry(entry: CheckpointEntry) -> Self {
        let mut steps = Self {
            leading: None,
            chunks: Vec::new().into_iter(),
            trailing: None,
        };
        steps.leading = match entry {
            CheckpointEntry::File(file) => {
                steps.chunks = file.objects.into_iter();
                steps.trailing = Some(Step::Finish);
                Some(Step::Open(FileStart {
                    path: file.path,
                    size: file.size,
                    executable: file.executable,
                }))
            }
            CheckpointEntry::Symlink { path, target } => Some(Step::Link { path, target }),
            CheckpointEntry::Remove { .. } => Some(Step::Remove),
            CheckpointEntry::Directory { .. } => None,
        };
        steps
    }
}

impl Iterator for EntrySteps {
    type Item = Step;

    fn next(&mut self) -> Option<Step> {
        self.leading
            .take()
            .or_else(|| self.chunks.next().map(Step::Chunk))
            .or_else(|| self.trailing.take())
    }
}

struct Batch {
    steps: Vec<Step>,
    ids: Vec<String>,
}

fn batch(source: &mut Peekable<impl Iterator<Item = Step>>) -> Batch {
    let mut steps = Vec::new();
    let mut ids = Vec::new();
    while steps.len() < ENTRIES {
        if ids.len() == CHUNKS && matches!(source.peek(), Some(Step::Chunk(_))) {
            break;
        }
        let Some(step) = source.next() else { break };
        if let Step::Chunk(id) = &step {
            ids.push(id.clone());
        }
        steps.push(step);
    }
    Batch { steps, ids }
}

/// Both turn and task restoration keep their own manifest and pin authorization.
/// An empty request renews custody before a local-only batch.
pub(super) async fn apply<S>(
    state: &mut S,
    destination: (PathBuf, (u32, u32)),
    entries: BTreeMap<String, CheckpointEntry>,
    permit: Option<&afr_agent::FileRecoveryPermit>,
    mut download: impl for<'s> FnMut(&'s mut S, &'s [String]) -> BoxFuture<'s, Result<Vec<Chunk>>>,
) -> Result<()> {
    let (path, owner) = destination;
    let root = open_empty(path, permit.cloned()).await?;
    let mut directories = entries.values().filter_map(|entry| match entry {
        CheckpointEntry::Directory { path } => Some(path),
        _ => None,
    });
    loop {
        let paths: Vec<_> = directories.by_ref().take(ENTRIES).cloned().collect();
        if paths.is_empty() {
            break;
        }
        download(state, &[]).await?;
        let root = Arc::clone(&root);
        let charged = permit.cloned();
        tokio::task::spawn_blocking(move || {
            let _charged = charged;
            write_directories(&root, owner, paths)
        })
        .await??;
    }

    let mut files = entries
        .into_values()
        .flat_map(EntrySteps::from_entry)
        .peekable();
    let mut writer = None;
    while files.peek().is_some() {
        let Batch { steps, ids } = batch(&mut files);
        let chunks = download(state, &ids).await?;
        let root = Arc::clone(&root);
        let charged = permit.cloned();
        writer = tokio::task::spawn_blocking(move || {
            let _charged = charged;
            write(&root, owner, writer, steps, chunks)
        })
        .await??;
    }
    if writer.is_some() {
        return Err(error::config("checkpoint ended with an unfinished file"));
    }
    Ok(())
}

async fn open_empty(
    path: PathBuf,
    permit: Option<afr_agent::FileRecoveryPermit>,
) -> Result<Arc<Workspace>> {
    tokio::task::spawn_blocking(move || {
        let _charged = permit;
        let root = Workspace::open(&path)?;
        root.require_empty()?;
        Ok(Arc::new(root))
    })
    .await?
}

fn write_directories(root: &Workspace, owner: (u32, u32), paths: Vec<String>) -> Result<()> {
    for path in paths {
        root.make_directory(&path, owner)?;
    }
    Ok(())
}

fn write(
    root: &Workspace,
    owner: (u32, u32),
    mut writer: Option<Restoring>,
    steps: Vec<Step>,
    chunks: Vec<Chunk>,
) -> Result<Option<Restoring>> {
    let mut chunks = chunks.into_iter();
    for step in steps {
        match step {
            Step::Open(file) => {
                writer = Some(root.make_file(&file.path, file.size, file.executable, owner)?);
            }
            Step::Chunk(_) => {
                let chunk = chunks.next().ok_or_else(|| {
                    error::config("checkpoint writer omitted an authorized chunk")
                })?;
                writer
                    .as_mut()
                    .ok_or_else(|| error::config("checkpoint chunk has no open file"))?
                    .append(chunk.expose())?;
            }
            Step::Finish => writer
                .take()
                .ok_or_else(|| error::config("checkpoint has no file to finish"))?
                .finish()?,
            Step::Link { path, target } => root.make_link(&path, &target, owner)?,
            Step::Remove => {
                return Err(error::config(
                    "checkpoint deletion survived overlay expansion",
                ));
            }
        }
    }
    Ok(writer)
}
