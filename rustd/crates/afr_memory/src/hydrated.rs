//! Memory held behind `agentsfleetd`, reached through its runner API.
//!
//! The window `agentsfleetd` hydrated at lease start is borrowed, never
//! copied. A store is held here until the supervisor pushes it, fenced, before
//! the report, so a run's memory calls cost `agentsfleetd` one hydrate and one
//! push per run — plus, when a recall finds fewer entries than it asked for,
//! at most [`RECALL_MISS_CAP`] searches past the window.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_wire::memory::{MAX_KEY_LEN, MAX_PUSH_BYTES, MemoryDelta};
use aho_corasick::AhoCorasick;
use garde::Validate as _;

use crate::error::{self, Result};
use crate::seed::{RECALL_MISS_CAP, Recalled, Seed};
use crate::{Forgotten, MemoryBackend};

/// A recall asked `agentsfleetd` past the window and could not get an answer.
const EVENT_MISS_FAILED: &str = "memory_recall_miss_failed";

/// One remembered entry, and whether the push still has to carry it.
#[derive(Debug)]
struct Entry<'run> {
    delta: MemoryDelta<'run>,
    pending: bool,
}

/// The hydrated window, the workspace's shared entries, and the run's stores.
#[derive(Debug, Default)]
pub struct Hydrated<'run> {
    /// The fleet's own entries, oldest first.
    entries: Vec<Entry<'run>>,
    /// What `agentsfleetd` hydrated beside them, read and never written.
    seed: Seed<'run>,
    /// What the pending entries charge against one push, kept as they come
    /// and go so a store never re-sums them.
    pending_bytes: usize,
    /// Keys this run stored or forgot. `agentsfleetd`'s copy under each is
    /// stale or forgotten, so an ask past the window never brings one back.
    superseded: HashSet<String>,
    /// Asks past the window so far. An atomic, because a recall takes
    /// `&self` and counting is the one thing it changes.
    misses: AtomicUsize,
}

impl<'run> Hydrated<'run> {
    /// The memory a run begins with, viewing `seed`'s window, which
    /// `agentsfleetd` sends newest first.
    #[must_use]
    pub fn new(seed: Seed<'run>) -> Self {
        let entries = seed
            .window
            .iter()
            .rev()
            .map(|delta| Entry {
                delta: delta.view(),
                pending: false,
            })
            .collect();
        Self {
            entries,
            seed,
            pending_bytes: 0,
            superseded: HashSet::new(),
            misses: AtomicUsize::new(0),
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

    /// The fleet's own entries, then the workspace's shared ones, newest first.
    fn newest_first(&self) -> impl Iterator<Item = Recalled<'_>> + '_ {
        let own = self
            .entries
            .iter()
            .rev()
            .map(|entry| Recalled::own(entry.delta.view()));
        own.chain(self.seed.shared.iter().map(Recalled::shared))
    }

    /// What the window holds for `query`, key matches first.
    fn matching(&self, query: &str, limit: usize) -> Result<Vec<Recalled<'_>>> {
        let matcher = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build([query])
            .map_err(error::query)?;
        let in_key = |found: &Recalled<'_>| matcher.is_match(found.key.as_ref());
        let in_content_only =
            |found: &Recalled<'_>| !in_key(found) && matcher.is_match(found.content.as_ref());
        Ok(self
            .newest_first()
            .filter(in_key)
            .chain(self.newest_first().filter(in_content_only))
            .take(limit)
            .collect())
    }

    /// Asks `agentsfleetd` for what the window missed, when the run has a
    /// seam to ask through and asks left; `None` answers from the window.
    async fn ask(&self, query: &str, limit: usize) -> Option<Vec<Recalled<'static>>> {
        let recall = self.seed.recall?;
        let asked = self.misses.fetch_add(1, Ordering::Relaxed);
        if asked >= RECALL_MISS_CAP {
            return None;
        }
        match recall.recall(query, limit).await {
            Ok(found) => {
                // Only the fleet's own entries: another fleet's shared entry
                // under the same key is one this run never stored or forgot.
                let own = found
                    .memory
                    .into_iter()
                    .filter(|delta| !self.superseded.contains(delta.key.as_ref()))
                    .map(Recalled::own);
                Some(
                    own.chain(found.shared.into_iter().map(Recalled::shared_owned))
                        .collect(),
                )
            }
            Err(failure) => {
                let code = failure.code().as_str();
                tracing::warn!(
                    error_code = code,
                    event = EVENT_MISS_FAILED,
                    "a recall past the window got no answer; the run answers from its window"
                );
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl MemoryBackend for Hydrated<'_> {
    async fn store(&mut self, entry: MemoryDelta<'static>) -> Result<()> {
        entry.validate().map_err(error::malformed)?;
        if entry.visibility.is_workspace() && !self.seed.publish {
            return Err(error::not_granted());
        }
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
        self.superseded.insert(entry.key.to_string());
        self.pending_bytes = needed;
        self.entries.push(Entry {
            delta: entry,
            pending: true,
        });
        Ok(())
    }

    async fn recall<'m>(&'m self, query: &str, limit: usize) -> Result<Vec<Recalled<'m>>> {
        let mut found = self.matching(query, limit)?;
        if found.len() >= limit {
            return Ok(found);
        }
        if let Some(asked) = self.ask(query, limit).await {
            for more in asked {
                if found.len() >= limit {
                    break;
                }
                if !found.iter().any(|held| held.same_entry(&more)) {
                    found.push(more);
                }
            }
        }
        Ok(found)
    }

    async fn list<'m>(&'m self, category: Option<&str>) -> Result<Vec<Recalled<'m>>> {
        Ok(self
            .newest_first()
            .filter(|found| category.is_none_or(|wanted| found.category == wanted))
            .collect())
    }

    async fn forget(&mut self, key: &str) -> Result<Forgotten> {
        // A key past the wire bound was never stored, because the daemon
        // refuses one, so there is no durable copy to hide and nothing to hold.
        if key.len() > MAX_KEY_LEN {
            return Ok(Forgotten::Unknown);
        }
        // Recorded even when the window held nothing under the key: the
        // durable copy can sit past the window, and the run asked it gone.
        self.superseded.insert(key.to_owned());
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
