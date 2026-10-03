//! Memory held behind `agentsfleetd`, reached through its runner API.
//!
//! The window `agentsfleetd` hydrated at lease start is borrowed, never
//! copied. A store is held here until the supervisor pushes it, fenced, before
//! the report, so a run's memory calls cost `agentsfleetd` nothing: one hydrate
//! and one push per run, however often the model recalls.

use afd_wire::memory::{MAX_PUSH_BYTES, MemoryDelta};
use aho_corasick::AhoCorasick;
use garde::Validate as _;

use crate::error::{self, Result};
use crate::{Forgotten, MemoryBackend};

/// One remembered entry, and whether the push still has to carry it.
#[derive(Debug)]
struct Entry<'run> {
    delta: MemoryDelta<'run>,
    pending: bool,
}

/// The hydrated window and the run's stores, oldest entry first.
#[derive(Debug, Default)]
pub struct Hydrated<'run> {
    entries: Vec<Entry<'run>>,
    /// What the pending entries charge against one push, kept as they come
    /// and go so a store never re-sums them.
    pending_bytes: usize,
}

impl<'run> Hydrated<'run> {
    /// The memory a run begins with, viewing the hydrated `window`, which
    /// `agentsfleetd` sends newest first.
    #[must_use]
    pub fn new(window: &'run [MemoryDelta<'run>]) -> Self {
        let entries = window
            .iter()
            .rev()
            .map(|delta| Entry {
                delta: delta.view(),
                pending: false,
            })
            .collect();
        Self {
            entries,
            pending_bytes: 0,
        }
    }

    /// Removes the entry under `key`, and its charge when it was pending.
    fn remove(&mut self, key: &str) -> Option<Entry<'run>> {
        let at = self
            .entries
            .iter()
            .position(|entry| entry.delta.key == key)?;
        let removed = self.entries.remove(at);
        if removed.pending {
            self.pending_bytes = self.pending_bytes.saturating_sub(removed.delta.bytes());
        }
        Some(removed)
    }

    fn newest_first(&self) -> impl Iterator<Item = &MemoryDelta<'run>> {
        self.entries.iter().rev().map(|entry| &entry.delta)
    }
}

#[async_trait::async_trait]
impl MemoryBackend for Hydrated<'_> {
    async fn store(&mut self, entry: MemoryDelta<'static>) -> Result<()> {
        entry.validate().map_err(error::malformed)?;
        let replaced = self
            .entries
            .iter()
            .find(|kept| kept.pending && kept.delta.key == entry.key)
            .map_or(0, |kept| kept.delta.bytes());
        let needed = self.pending_bytes.saturating_sub(replaced) + entry.bytes();
        if needed > MAX_PUSH_BYTES {
            return Err(error::full(needed));
        }
        self.remove(&entry.key);
        self.pending_bytes = needed;
        self.entries.push(Entry {
            delta: entry,
            pending: true,
        });
        Ok(())
    }

    async fn recall<'m>(&'m self, query: &str, limit: usize) -> Result<Vec<MemoryDelta<'m>>> {
        let matcher = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build([query])
            .map_err(error::query)?;
        let in_key = |delta: &&MemoryDelta<'_>| matcher.is_match(delta.key.as_ref());
        let in_content_only =
            |delta: &&MemoryDelta<'_>| !in_key(delta) && matcher.is_match(delta.content.as_ref());
        Ok(self
            .newest_first()
            .filter(in_key)
            .chain(self.newest_first().filter(in_content_only))
            .take(limit)
            .map(MemoryDelta::view)
            .collect())
    }

    async fn list<'m>(&'m self, category: Option<&str>) -> Result<Vec<MemoryDelta<'m>>> {
        Ok(self
            .newest_first()
            .filter(|delta| category.is_none_or(|wanted| delta.category == wanted))
            .map(MemoryDelta::view)
            .collect())
    }

    async fn forget(&mut self, key: &str) -> Result<Forgotten> {
        Ok(self
            .remove(key)
            .map_or(Forgotten::Unknown, |_| Forgotten::ForThisRun))
    }

    fn into_pending(self: Box<Self>) -> Vec<MemoryDelta<'static>> {
        self.entries
            .into_iter()
            .filter(|entry| entry.pending)
            .map(|entry| entry.delta.into_owned())
            .collect()
    }

    fn pending(&self) -> Vec<MemoryDelta<'_>> {
        self.entries
            .iter()
            .filter(|entry| entry.pending)
            .map(|entry| entry.delta.view())
            .collect()
    }
}
