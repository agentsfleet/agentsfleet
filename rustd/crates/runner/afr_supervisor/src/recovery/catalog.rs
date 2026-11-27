//! A save's manifest, built from the changes a pin yields: each entry listed,
//! each chunk hashed once, then only the missing chunks uploaded.

use super::files;
use super::tree::ChangeSource;
#[cfg(test)]
use super::{
    files::Workspace,
    names::{EveryNameItsOwnFile, SecondNames},
    tree::walk,
};
use crate::{Result, error};
use af_chunk_store::{Change, ChunkKey};
use af_wire::durable_task::{CheckpointEntry, CheckpointPage, CheckpointPlaintext};
use futures_util::StreamExt as _;
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// One history chunk's key and its bytes.
pub(super) type HistoryChunk<'a> = (&'a str, Zeroizing<Vec<u8>>);

const PLACEHOLDER: &str = "00000000-0000-7000-8000-000000000000";

/// One distinct chunk: what is registered for it, and where history's bytes
/// sit when it is history. A file chunk's bytes are read again from the pin.
#[derive(Clone)]
pub(super) struct Chunk {
    pub(super) descriptor: CheckpointPlaintext,
    history: Option<(usize, usize)>,
}

impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointChunk")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(super) struct Catalog {
    pub(super) chunks: BTreeMap<String, Chunk>,
    entries: BTreeMap<String, CheckpointEntry>,
    file_chunks: BTreeMap<String, Vec<String>>,
    history_chunks: Vec<String>,
    history: Arc<Zeroizing<Vec<u8>>>,
    /// The file whose chunks the changes are on, and its size.
    open_file: Option<(String, u64)>,
    pub(super) logical_bytes: u64,
    pub(super) entry_count: u32,
}

impl Catalog {
    /// The catalog of everything `source` yields, `history` beside it, and a
    /// removal for each of `base_paths` the source no longer lists.
    ///
    /// # Errors
    /// A change could not be read, the changes are not a whole tree, or the
    /// pages it lays out exceed their bounds.
    pub(super) async fn read(
        source: Option<&dyn ChangeSource>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
        stop: &CancellationToken,
    ) -> Result<Self> {
        let mut catalog = Self::empty();
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
        catalog.finish(history, base_paths)
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
        let mut catalog = Self::empty();
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
        catalog.finish(history, base_paths)
    }

    fn empty() -> Self {
        Self {
            chunks: BTreeMap::new(),
            entries: BTreeMap::new(),
            file_chunks: BTreeMap::new(),
            history_chunks: Vec::new(),
            history: Arc::new(Zeroizing::new(Vec::new())),
            open_file: None,
            logical_bytes: 0,
            entry_count: 0,
        }
    }

    fn add_entry(&mut self, entry: CheckpointEntry) -> Result<()> {
        self.close_file()?;
        let path = entry.path().to_owned();
        files::valid_path(&path)?;
        if let CheckpointEntry::File(file) = &entry {
            self.logical_bytes = self
                .logical_bytes
                .checked_add(file.size)
                .ok_or_else(|| error::capture_limit("checkpoint size overflow"))?;
            self.file_chunks.insert(path.clone(), Vec::new());
            self.open_file = Some((path.clone(), file.size));
        }
        self.entry_count = self
            .entry_count
            .checked_add(1)
            .ok_or_else(|| error::capture_limit("checkpoint entry count overflow"))?;
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
    fn close_file(&mut self) -> Result<()> {
        let Some((path, size)) = self.open_file.take() else {
            return Ok(());
        };
        let read = self.file_chunks.get(&path).map_or(0, Vec::len);
        if crate::byte_count(read) != size.div_ceil(files::CHUNK) {
            return Err(error::config("checkpoint file is incomplete"));
        }
        Ok(())
    }

    fn finish(mut self, history: Zeroizing<Vec<u8>>, base_paths: Vec<String>) -> Result<Self> {
        self.close_file()?;
        for path in base_paths {
            if !self.entries.contains_key(&path) {
                self.entries
                    .insert(path.clone(), CheckpointEntry::Remove { path });
            }
        }
        self.add_history(history)?;
        // Validate the complete page layout before allocating any native plan.
        self.pages(None, af_wire::durable_task::CHECKPOINT_PAGE_BYTES)?;
        Ok(self)
    }

    fn add_history(&mut self, history: Zeroizing<Vec<u8>>) -> Result<()> {
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

    /// The history chunks, by key, with their bytes.
    pub(super) fn history(&self) -> Result<Vec<HistoryChunk<'_>>> {
        self.history_chunks
            .iter()
            .map(|key| {
                let (start, end) = self
                    .chunks
                    .get(key)
                    .and_then(|chunk| chunk.history)
                    .ok_or_else(|| error::config("checkpoint history source is absent"))?;
                let bytes = self
                    .history
                    .get(start..end)
                    .ok_or_else(|| error::config("checkpoint history chunk range is invalid"))?;
                Ok((key.as_str(), Zeroizing::new(bytes.to_vec())))
            })
            .collect()
    }

    pub(super) fn history_root(&self, ids: &BTreeMap<String, String>) -> Result<String> {
        let mut history = Vec::new();
        for key in &self.history_chunks {
            let id = ids
                .get(key)
                .ok_or_else(|| error::config("checkpoint history registration is incomplete"))?;
            let chunk = self
                .chunks
                .get(key)
                .ok_or_else(|| error::config("checkpoint history source is absent"))?;
            history.push((
                id.as_str(),
                chunk.descriptor.sha256.as_str(),
                chunk.descriptor.size,
            ));
        }
        Ok(af_crypto::checkpoint_digest::history_root(history))
    }

    pub(super) fn pages(
        &self,
        ids: Option<&BTreeMap<String, String>>,
        ceiling: usize,
    ) -> Result<Vec<CheckpointPage>> {
        let mut pages = super::layout::Pages::new(ceiling)?;
        for (path, original) in &self.entries {
            pages.push(self.mapped_entry(path, original, ids)?)?;
        }
        let history = self
            .history_chunks
            .iter()
            .map(|key| {
                ids.map_or_else(
                    || Ok(PLACEHOLDER.to_owned()),
                    |ids| {
                        ids.get(key).cloned().ok_or_else(|| {
                            error::config("checkpoint history registration is incomplete")
                        })
                    },
                )
            })
            .collect::<Result<_>>()?;
        pages.finish(history)
    }

    fn mapped_entry(
        &self,
        path: &str,
        original: &CheckpointEntry,
        ids: Option<&BTreeMap<String, String>>,
    ) -> Result<CheckpointEntry> {
        let mut entry = original.clone();
        if let CheckpointEntry::File(file) = &mut entry {
            let keys = self
                .file_chunks
                .get(path)
                .ok_or_else(|| error::config("checkpoint file mapping is absent"))?;
            file.objects = match ids {
                None => vec![PLACEHOLDER.to_owned(); keys.len()],
                Some(ids) => keys
                    .iter()
                    .map(|key| {
                        ids.get(key).cloned().ok_or_else(|| {
                            error::config("checkpoint file registration is incomplete")
                        })
                    })
                    .collect::<Result<_>>()?,
            };
        }
        Ok(entry)
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
