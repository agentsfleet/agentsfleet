//! Two bounded passes hash frozen files, then upload only missing native chunks.

use super::files::{self, Entry, Identity, Workspace};
use crate::{Result, error};
use af_wire::task::{CheckpointEntry, CheckpointFile, CheckpointPage, CheckpointPlaintext};
use std::{collections::BTreeMap, sync::Arc};
use zeroize::Zeroizing;

const PLACEHOLDER: &str = "00000000-0000-7000-8000-000000000000";
const MAX_HISTORY: usize = 64 * 1024 * 1024;

#[derive(Clone)]
enum Source {
    File {
        root: Arc<Workspace>,
        path: String,
        identity: Identity,
        offset: u64,
    },
    History {
        bytes: Arc<Zeroizing<Vec<u8>>>,
        start: usize,
        end: usize,
    },
}

#[derive(Clone)]
pub(super) struct Chunk {
    pub(super) descriptor: CheckpointPlaintext,
    source: Source,
}

impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointChunk")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl Chunk {
    pub(super) fn read(&self) -> Result<Zeroizing<Vec<u8>>> {
        match &self.source {
            Source::File {
                root,
                path,
                identity,
                offset,
            } => root.chunk(path, identity, *offset),
            Source::History { bytes, start, end } => Ok(Zeroizing::new(
                bytes
                    .get(*start..*end)
                    .ok_or_else(|| error::config("checkpoint history chunk range is invalid"))?
                    .to_vec(),
            )),
        }
    }
}

#[derive(Debug)]
pub(super) struct Catalog {
    pub(super) chunks: BTreeMap<String, Chunk>,
    entries: BTreeMap<String, CheckpointEntry>,
    file_chunks: BTreeMap<String, Vec<String>>,
    history_chunks: Vec<String>,
    pub(super) logical_bytes: u64,
    pub(super) entry_count: u32,
}

impl Catalog {
    pub(super) fn build(
        root: Option<&Arc<Workspace>>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
        stop: &tokio_util::sync::CancellationToken,
    ) -> Result<Self> {
        if history.len() > MAX_HISTORY {
            return Err(error::config("checkpoint history exceeds its bound"));
        }
        let inventory = root
            .map(|root| root.inventory(stop))
            .transpose()?
            .unwrap_or_default();
        let mut catalog = Self {
            chunks: BTreeMap::new(),
            entries: BTreeMap::new(),
            file_chunks: BTreeMap::new(),
            history_chunks: Vec::new(),
            logical_bytes: 0,
            entry_count: u32::try_from(inventory.len())
                .map_err(|source| error::boundary("checkpoint entry count overflow", source))?,
        };
        for (path, entry) in &inventory {
            if stop.is_cancelled() {
                return Err(error::config("checkpoint inventory cancelled"));
            }
            catalog.logical_bytes = catalog
                .logical_bytes
                .checked_add(files::size(entry))
                .ok_or_else(|| error::config("checkpoint size overflow"))?;
            let wire = match entry {
                Entry::Directory => CheckpointEntry::Directory { path: path.clone() },
                Entry::Symlink { target } => CheckpointEntry::Symlink {
                    path: path.clone(),
                    target: target.clone(),
                },
                Entry::File {
                    identity,
                    executable,
                } => {
                    let root =
                        root.ok_or_else(|| error::config("checkpoint file has no workspace"))?;
                    let keys = catalog.file(root, path, identity, files::size(entry), stop)?;
                    let file = CheckpointEntry::File(CheckpointFile {
                        path: path.clone(),
                        executable: *executable,
                        size: files::size(entry),
                        objects: vec![PLACEHOLDER.to_owned(); keys.len()],
                    });
                    catalog.file_chunks.insert(path.clone(), keys);
                    file
                }
            };
            catalog.entries.insert(path.clone(), wire);
        }
        for path in base_paths {
            if !inventory.contains_key(&path) {
                catalog
                    .entries
                    .insert(path.clone(), CheckpointEntry::Remove { path });
            }
        }
        let history = Arc::new(history);
        let chunk_bytes = usize::try_from(files::CHUNK)
            .map_err(|source| error::boundary("checkpoint chunk size overflow", source))?;
        for (index, chunk) in history.chunks(chunk_bytes).enumerate() {
            let start = index * chunk_bytes;
            let end = start + chunk.len();
            let descriptor = descriptor(chunk, "history");
            let key = key(&descriptor);
            catalog.chunks.entry(key.clone()).or_insert_with(|| Chunk {
                descriptor,
                source: Source::History {
                    bytes: Arc::clone(&history),
                    start,
                    end,
                },
            });
            catalog.history_chunks.push(key);
        }
        // Validate the complete page layout before allocating any native plan.
        catalog.pages(None)?;
        Ok(catalog)
    }

    fn file(
        &mut self,
        root: &Arc<Workspace>,
        path: &str,
        identity: &Identity,
        size: u64,
        stop: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        let chunk_bytes = usize::try_from(files::CHUNK)
            .map_err(|source| error::boundary("checkpoint chunk size overflow", source))?;
        for offset in (0..size).step_by(chunk_bytes) {
            if stop.is_cancelled() {
                return Err(error::config("checkpoint hashing cancelled"));
            }
            let bytes = root.chunk(path, identity, offset)?;
            let descriptor = descriptor(&bytes, "file");
            let key = key(&descriptor);
            self.chunks.entry(key.clone()).or_insert_with(|| Chunk {
                descriptor,
                source: Source::File {
                    root: Arc::clone(root),
                    path: path.to_owned(),
                    identity: identity.clone(),
                    offset,
                },
            });
            keys.push(key);
        }
        Ok(keys)
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
    ) -> Result<Vec<CheckpointPage>> {
        let initial = CheckpointPage {
            page_number: 0,
            entries: Vec::new(),
            history_objects: Vec::new(),
        };
        let mut page_bytes = serde_json::to_vec(&initial).map_err(error::encode)?.len();
        let mut pages = vec![initial];
        for (path, original) in &self.entries {
            let mut entry = original.clone();
            if let (Some(ids), CheckpointEntry::File(file)) = (ids, &mut entry) {
                file.objects = self
                    .file_chunks
                    .get(path)
                    .ok_or_else(|| error::config("checkpoint file mapping is absent"))?
                    .iter()
                    .map(|key| {
                        ids.get(key).cloned().ok_or_else(|| {
                            error::config("checkpoint file registration is incomplete")
                        })
                    })
                    .collect::<Result<_>>()?;
            }
            // Each entry is encoded once per layout pass. Re-encoding the whole
            // growing page per insertion makes large repositories quadratic.
            let entry_bytes = serde_json::to_vec(&entry).map_err(error::encode)?.len();
            let page = pages
                .last()
                .ok_or_else(|| error::config("checkpoint page is absent"))?;
            let separator = usize::from(!page.entries.is_empty());
            if page.entries.len() == 4096
                || page_bytes + separator + entry_bytes > af_wire::task::CHECKPOINT_PAGE_BYTES
            {
                if page.entries.is_empty()
                    || pages.len()
                        == usize::try_from(af_wire::task::CHECKPOINT_PAGES).unwrap_or(usize::MAX)
                {
                    return Err(error::config("checkpoint manifest exceeds its page bound"));
                }
                let page_number = u32::try_from(pages.len())
                    .map_err(|source| error::boundary("checkpoint page count overflow", source))?;
                let page = CheckpointPage {
                    page_number,
                    entries: Vec::new(),
                    history_objects: Vec::new(),
                };
                page_bytes = serde_json::to_vec(&page).map_err(error::encode)?.len();
                pages.push(page);
            }
            let page = pages
                .last_mut()
                .ok_or_else(|| error::config("checkpoint page is absent"))?;
            page_bytes += usize::from(!page.entries.is_empty()) + entry_bytes;
            page.entries.push(entry);
        }
        let page = pages
            .last_mut()
            .ok_or_else(|| error::config("checkpoint page is absent"))?;
        page.history_objects =
            self.history_chunks
                .iter()
                .map(|key| match ids {
                    Some(ids) => ids.get(key).cloned().ok_or_else(|| {
                        error::config("checkpoint history registration is incomplete")
                    }),
                    None => Ok(PLACEHOLDER.to_owned()),
                })
                .collect::<Result<_>>()?;
        if !af_wire::decision::serialized_within(page, af_wire::task::CHECKPOINT_PAGE_BYTES) {
            let history_objects = std::mem::take(&mut page.history_objects);
            pages.push(CheckpointPage {
                page_number: u32::try_from(pages.len())
                    .map_err(|source| error::boundary("checkpoint page count overflow", source))?,
                entries: Vec::new(),
                history_objects,
            });
        }
        if pages.len() > usize::try_from(af_wire::task::CHECKPOINT_PAGES).unwrap_or(usize::MAX) {
            return Err(error::config("checkpoint manifest exceeds its page bound"));
        }
        for page in &pages {
            garde::Validate::validate(page)?;
            if !af_wire::decision::serialized_within(page, af_wire::task::CHECKPOINT_PAGE_BYTES) {
                return Err(error::config("checkpoint entry exceeds a manifest page"));
            }
        }
        Ok(pages)
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
