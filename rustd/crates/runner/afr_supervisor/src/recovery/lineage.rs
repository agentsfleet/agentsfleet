//! The tree an owner's last committed generation resolves to, and the rule
//! that keeps its delta chain shorter than a full manifest.
//!
//! ```text
//!   lineage(head) ⊕ delta ─► view            the tree the new generation resolves to
//!   chain pages + delta pages > full pages   ─► write the view whole, no parent
//!   otherwise                                ─► write the delta, parent = head
//! ```
//!
//! Both page counts come from the save's own layout, so the threshold is the
//! data's, never a chosen length. A full manifest registers every chunk the
//! view names, so its generation holds an edge to each object it needs and
//! the chain it supersedes can go.

use std::collections::BTreeMap;

use af_chunk_store::Resolved;
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointEntry, CheckpointPlaintext};
use zeroize::Zeroizing;

use super::catalog::{Catalog, Chunk};
use crate::Result;

/// The tree one committed generation resolves to, held after its commit.
#[derive(Debug, Clone)]
pub(crate) struct Lineage {
    head: Uuid7,
    view: View,
    /// The pages every delta since the chain's full manifest wrote.
    delta_pages: usize,
}

impl Lineage {
    /// The generation the tree is.
    pub(crate) const fn head(&self) -> &Uuid7 {
        &self.head
    }
}

/// A workspace tree as entries and each file's chunks; it holds no removal.
#[derive(Debug, Clone, Default)]
pub(crate) struct View {
    pub(super) entries: BTreeMap<String, CheckpointEntry>,
    file_chunks: BTreeMap<String, Vec<String>>,
    descriptors: BTreeMap<String, CheckpointPlaintext>,
}

impl View {
    /// The tree a catalog of the whole tree lists, before any removal of a
    /// base path is added.
    pub(super) fn of(catalog: &Catalog) -> Self {
        let descriptors = catalog
            .file_chunks
            .values()
            .flatten()
            .filter_map(|key| {
                let chunk = catalog.chunks.get(key)?;
                Some((key.clone(), chunk.descriptor.clone()))
            })
            .collect();
        Self {
            entries: catalog.entries.clone(),
            file_chunks: catalog.file_chunks.clone(),
            descriptors,
        }
    }

    /// `delta` laid over `self`: each changed entry replaces its path, each
    /// removal drops its path and what is below it.
    fn applied(&self, delta: &Catalog) -> Result<Self> {
        let mut tree = Resolved::from(self.entries.clone());
        for entry in delta.entries.values() {
            tree.apply(entry.clone())?;
        }
        let entries = tree.into_entries();
        let mut view = Self::default();
        for (path, entry) in &entries {
            if !matches!(entry, CheckpointEntry::File(_)) {
                continue;
            }
            let (keys, from) = match delta.file_chunks.get(path) {
                Some(keys) => (keys, None),
                None => (
                    self.file_chunks.get(path).ok_or_else(|| {
                        crate::error::config("checkpoint lineage lost a file's chunks")
                    })?,
                    Some(self),
                ),
            };
            for key in keys {
                let descriptor = from
                    .map_or_else(
                        || delta.chunks.get(key).map(|chunk| chunk.descriptor.clone()),
                        |parent| parent.descriptors.get(key).cloned(),
                    )
                    .ok_or_else(|| crate::error::config("checkpoint lineage lost a chunk"))?;
                view.descriptors.insert(key.clone(), descriptor);
            }
            view.file_chunks.insert(path.clone(), keys.clone());
        }
        view.entries = entries;
        Ok(view)
    }
}

impl Catalog {
    /// Chooses what a save over `lineage` writes, from the delta `self` holds:
    /// the delta, with `lineage`'s head its parent, unless the chain's delta
    /// pages with this one's would outnumber a full manifest's, when it writes
    /// the whole tree with no parent.
    ///
    /// # Errors
    /// The delta names a file the tree does not hold, or a layout exceeds its
    /// bounds.
    pub(super) fn rebase(
        mut self,
        lineage: &Lineage,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
        ceiling: usize,
    ) -> Result<Self> {
        let view = lineage.view.applied(&self)?;
        let chunks = view
            .descriptors
            .iter()
            .map(|(key, descriptor)| (key.clone(), Chunk::file(descriptor.clone())))
            .collect();
        let full = Self::of_files(view.entries.clone(), view.file_chunks.clone(), chunks).finish(
            history.clone(),
            base_paths,
            ceiling,
        )?;
        let full_pages = full.pages(None, ceiling)?.len();
        self.add_history(history)?;
        let delta_pages = self.pages(None, ceiling)?.len();
        let chain_pages = lineage.delta_pages + delta_pages;
        let mut chosen = if chain_pages > full_pages {
            full
        } else {
            self.parent = Some(lineage.head.clone());
            self.chain_pages = chain_pages;
            self
        };
        chosen.view = view;
        chosen.count()?;
        Ok(chosen)
    }

    /// The tree this save resolves to, once the daemon committed it as `head`.
    pub(super) fn lineage(&self, head: Uuid7) -> Lineage {
        Lineage {
            head,
            view: self.view.clone(),
            delta_pages: self.chain_pages,
        }
    }
}

#[cfg(test)]
#[path = "lineage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lineage_walk_tests.rs"]
mod walk_tests;
