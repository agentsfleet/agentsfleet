//! A save's manifest, built from the changes a pin yields: each entry listed,
//! each chunk hashed once, then only the missing chunks uploaded.
//!
//! A pin that read only what changed since its parent yields a delta, which
//! [`super::lineage`] lays over the parent's tree before the save chooses
//! between writing it and writing the whole tree again.

use super::files;
use super::lineage::{Lineage, View};
use super::tree::ChangeSource;
#[cfg(test)]
use super::{
    files::Workspace,
    names::{EveryNameItsOwnFile, SecondNames},
    tree::walk,
};
use crate::{Result, error};
use af_chunk_store::{Change, ChunkKey};
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointEntry, CheckpointPlaintext};
use futures_util::StreamExt as _;
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

mod pages;

/// One history chunk's key and its bytes.
pub(super) type HistoryChunk<'a> = (&'a str, Zeroizing<Vec<u8>>);

pub(super) const PLACEHOLDER: &str = "00000000-0000-7000-8000-000000000000";

/// One distinct chunk: what is registered for it, and where history's bytes
/// sit when it is history. A file chunk's bytes are read again from the pin.
#[derive(Clone)]
pub(super) struct Chunk {
    pub(super) descriptor: CheckpointPlaintext,
    pub(super) history: Option<(usize, usize)>,
}

impl Chunk {
    /// A file chunk, its bytes read from the pin.
    pub(super) const fn file(descriptor: CheckpointPlaintext) -> Self {
        Self {
            descriptor,
            history: None,
        }
    }
}

impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointChunk")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(super) struct Catalog {
    /// Every distinct chunk the save registers.
    pub(super) chunks: BTreeMap<String, Chunk>,
    /// What the save's pages list: the whole tree, or a delta.
    pub(super) entries: BTreeMap<String, CheckpointEntry>,
    pub(super) file_chunks: BTreeMap<String, Vec<String>>,
    pub(super) history_chunks: Vec<String>,
    pub(super) history: Arc<Zeroizing<Vec<u8>>>,
    /// The file whose chunks the changes are on, and its size.
    open_file: Option<(String, u64)>,
    /// The resolved tree's bytes and entries, whatever the pages list.
    pub(super) logical_bytes: u64,
    pub(super) entry_count: u32,
    /// The generation the pages are a delta against; none for a full tree.
    pub(super) parent: Option<Uuid7>,
    /// The pages every delta since the chain's full manifest wrote, this
    /// save's among them; zero for a full tree.
    pub(super) chain_pages: usize,
    /// The tree the save resolves to, for the next save to build on.
    pub(super) view: View,
}

impl Catalog {
    /// A catalog of the whole tree `entries` lists, each file's chunks
    /// already hashed: what a save writes when it writes a delta's tree whole.
    pub(super) fn of_files(
        entries: BTreeMap<String, CheckpointEntry>,
        file_chunks: BTreeMap<String, Vec<String>>,
        chunks: BTreeMap<String, Chunk>,
    ) -> Self {
        Self {
            chunks,
            entries,
            file_chunks,
            ..Self::default()
        }
    }

    /// The catalog of everything `source` yields, `history` beside it, and a
    /// removal for each of `base_paths` the source no longer lists.
    ///
    /// # Errors
    /// A change could not be read, the changes are not a whole tree, or the
    /// pages it lays out exceed their bounds.
    pub(super) async fn read(
        source: Option<&dyn ChangeSource>,
        history: Zeroizing<Vec<u8>>,
        base: (Vec<String>, Option<&Lineage>),
        ceiling: usize,
        stop: &CancellationToken,
    ) -> Result<Self> {
        let mut catalog = Self::default();
        if let Some(source) = source {
            let mut changes = source.changes();
            while let Some(change) = changes.next().await {
                if stop.is_cancelled() {
                    return Err(error::config("checkpoint inventory cancelled"));
                }
                match change? {
                    Change::Entry(entry) => catalog.add_entry(entry)?,
                    Change::Chunk { key, bytes } => {
                        let descriptor =
                            tokio::task::spawn_blocking(move || descriptor(&bytes, "file")).await?;
                        catalog.add_chunk(&key, descriptor)?;
                    }
                }
            }
        }
        let (base_paths, lineage) = base;
        match source.and_then(ChangeSource::parent) {
            Some(parent) => {
                let lineage = lineage
                    .filter(|lineage| lineage.head() == parent)
                    .ok_or_else(|| error::config("checkpoint delta names no generation held"))?;
                catalog.close_file()?;
                catalog.rebase(lineage, history, base_paths, ceiling)
            }
            None => catalog.finish(history, base_paths, ceiling),
        }
    }

    /// The catalog of the tree `root` holds, walked in place.
    #[cfg(test)]
    pub(super) fn build(
        root: Option<&Arc<Workspace>>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
        stop: &CancellationToken,
    ) -> Result<Self> {
        Self::build_naming(root, history, base_paths, stop, &mut EveryNameItsOwnFile)
    }

    /// As [`Self::build`], asking `names` once per file how its name is saved.
    #[cfg(test)]
    pub(super) fn build_naming(
        root: Option<&Arc<Workspace>>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
        stop: &CancellationToken,
        names: &mut dyn SecondNames,
    ) -> Result<Self> {
        let mut catalog = Self::default();
        if let Some(root) = root {
            walk(root, stop, names, &mut |change| {
                match change {
                    Change::Entry(entry) => catalog.add_entry(entry)?,
                    Change::Chunk { key, bytes } => {
                        catalog.add_chunk(&key, descriptor(&bytes, "file"))?;
                    }
                }
                Ok(true)
            })?;
        }
        catalog.finish(
            history,
            base_paths,
            af_wire::durable_task::CHECKPOINT_PAGE_BYTES,
        )
    }

    fn add_entry(&mut self, entry: CheckpointEntry) -> Result<()> {
        self.close_file()?;
        let path = entry.path().to_owned();
        files::valid_path(&path)?;
        if let CheckpointEntry::File(file) = &entry {
            self.file_chunks.insert(path.clone(), Vec::new());
            self.open_file = Some((path.clone(), file.size));
        }
        if self.entries.insert(path, entry).is_some() {
            return Err(error::config("checkpoint path repeated"));
        }
        Ok(())
    }

    fn add_chunk(&mut self, at: &ChunkKey, descriptor: CheckpointPlaintext) -> Result<()> {
        let ChunkKey::File { path, index } = at;
        let keys = self
            .open_file
            .as_ref()
            .filter(|(open, _)| open == path)
            .and_then(|(open, _)| self.file_chunks.get_mut(open))
            .ok_or_else(|| error::config("checkpoint chunk names no file being read"))?;
        if crate::byte_count(keys.len()) != *index {
            return Err(error::config("checkpoint chunks arrived out of order"));
        }
        let key = key(&descriptor);
        keys.push(key.clone());
        self.chunks.entry(key).or_insert(Chunk {
            descriptor,
            history: None,
        });
        Ok(())
    }

    /// Ends the file whose chunks were being read: it holds every chunk its
    /// size says it does.
    pub(super) fn close_file(&mut self) -> Result<()> {
        let Some((path, size)) = self.open_file.take() else {
            return Ok(());
        };
        let read = self.file_chunks.get(&path).map_or(0, Vec::len);
        if crate::byte_count(read) != size.div_ceil(files::CHUNK) {
            return Err(error::config("checkpoint file is incomplete"));
        }
        Ok(())
    }

    /// Completes a catalog of the whole tree: `history` beside it, and a
    /// removal for each of `base_paths` the tree no longer lists.
    pub(super) fn finish(
        mut self,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
        ceiling: usize,
    ) -> Result<Self> {
        self.close_file()?;
        self.view = View::of(&self);
        self.count()?;
        for path in base_paths {
            if !self.entries.contains_key(&path) {
                self.entries
                    .insert(path.clone(), CheckpointEntry::Remove { path });
            }
        }
        self.add_history(history)?;
        // Validate the complete page layout before allocating any native plan.
        self.pages(None, ceiling)?;
        Ok(self)
    }

    /// Takes the resolved tree's bytes and entries from the view.
    pub(super) fn count(&mut self) -> Result<()> {
        self.logical_bytes = self
            .view
            .entries
            .values()
            .try_fold(0_u64, |sum, entry| match entry {
                CheckpointEntry::File(file) => sum.checked_add(file.size),
                _ => Some(sum),
            })
            .ok_or_else(|| error::capture_limit("checkpoint size overflow"))?;
        self.entry_count = u32::try_from(self.view.entries.len())
            .map_err(|_overflow| error::capture_limit("checkpoint entry count overflow"))?;
        Ok(())
    }

    pub(super) fn add_history(&mut self, history: Zeroizing<Vec<u8>>) -> Result<()> {
        self.history = Arc::new(history);
        let chunk_bytes = usize::try_from(files::CHUNK)
            .map_err(|source| error::boundary("checkpoint chunk size overflow", source))?;
        for (index, chunk) in self.history.chunks(chunk_bytes).enumerate() {
            let start = index * chunk_bytes;
            let descriptor = descriptor(chunk, "history");
            let key = key(&descriptor);
            self.chunks.entry(key.clone()).or_insert(Chunk {
                descriptor,
                history: Some((start, start + chunk.len())),
            });
            self.history_chunks.push(key);
        }
        Ok(())
    }

    /// The distinct chunk the `index`th chunk of `path` is.
    pub(super) fn file_chunk(&self, path: &str, index: u64) -> Option<&str> {
        let index = usize::try_from(index).ok()?;
        self.file_chunks.get(path)?.get(index).map(String::as_str)
    }
}

fn descriptor(bytes: &[u8], kind: &str) -> CheckpointPlaintext {
    CheckpointPlaintext {
        sha256: af_core::etag::sha256_hex(bytes),
        size: crate::byte_count(bytes.len()),
        kind: kind.to_owned(),
    }
}

fn key(descriptor: &CheckpointPlaintext) -> String {
    format!(
        "{}:{}:{}",
        descriptor.kind, descriptor.sha256, descriptor.size
    )
}
