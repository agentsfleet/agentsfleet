//! A delta chain resolved into the tree it saved.
//!
//! A generation's manifest is either full or a delta against its parent: the
//! entries that changed and a removal for each path that went. A restore
//! applies the chain's manifests in order, its full one first, and the tree
//! that results is what a full manifest of the last generation lists.
//!
//! ```text
//!   full ─► delta ─► delta ─► …     each applied over the tree before it
//!   Remove { path }                 drops the path and everything below it
//! ```

use std::collections::BTreeMap;

use af_wire::durable_task::CheckpointEntry;

use crate::error::{Result, refused};

/// The most entries a resolved tree holds: the daemon's expanded-tree bound,
/// which every manifest it accepted already met.
pub const MAX_ENTRIES: usize = 400_000;

/// A tree built by applying manifest entries in chain order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    entries: BTreeMap<String, CheckpointEntry>,
}

impl From<BTreeMap<String, CheckpointEntry>> for Resolved {
    /// The tree an earlier resolution left, to apply more entries over.
    fn from(entries: BTreeMap<String, CheckpointEntry>) -> Self {
        Self { entries }
    }
}

impl Resolved {
    /// Applies one entry: a removal drops its path and everything below it,
    /// any other entry replaces what its path held.
    ///
    /// # Errors
    /// The tree outgrew [`MAX_ENTRIES`].
    pub fn apply(&mut self, entry: CheckpointEntry) -> Result<()> {
        if let CheckpointEntry::Remove { path } = entry {
            self.remove(&path);
            return Ok(());
        }
        self.entries.insert(entry.path().to_owned(), entry);
        if self.entries.len() > MAX_ENTRIES {
            return Err(refused("checkpoint overlay exceeds entry bound"));
        }
        Ok(())
    }

    /// Drops `path` and everything below it.
    pub fn remove(&mut self, path: &str) {
        self.entries.remove(path);
        let prefix = format!("{path}/");
        let below: Vec<_> = self
            .entries
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
            .map(|(path, _)| path.clone())
            .collect();
        for path in below {
            self.entries.remove(&path);
        }
    }

    /// The resolved entries, by path.
    #[must_use]
    pub const fn entries(&self) -> &BTreeMap<String, CheckpointEntry> {
        &self.entries
    }

    /// The resolved entries, by path, taken.
    #[must_use]
    pub fn into_entries(self) -> BTreeMap<String, CheckpointEntry> {
        self.entries
    }
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;
