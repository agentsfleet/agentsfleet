//! Memory held behind `agentsfleetd`, reached through its runner API.
//!
//! The window `agentsfleetd` loaded at run start is borrowed, never
//! copied. A store is held here until the supervisor pushes it, fenced, before
//! the report, so a run's memory calls cost `agentsfleetd` one load and one
//! push per run — plus, when a recall finds fewer entries than it asked for,
//! at most [`RECALL_MISS_CAP`] searches past the window.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use af_wire::memory::{MAX_KEY_LEN, MAX_PUSH_BYTES, MemoryDelta};
use aho_corasick::AhoCorasick;
use garde::Validate as _;

use crate::error::{self, Result};
use crate::seed::{RECALL_MISS_CAP, Recalled, Seed};
use crate::{Forgotten, MemoryRecovery};

/// A recall asked `agentsfleetd` past the window and could not get an answer.
const EVENT_MISS_FAILED: &str = "memory_recall_miss_failed";

/// One remembered entry, and whether the push still has to carry it.
#[derive(Debug)]
struct Entry<'run> {
    delta: MemoryDelta<'run>,
    pending: bool,
}

/// The loaded window, the workspace's shared entries, and the run's stores.
#[derive(Debug, Default)]
pub struct Loaded<'run> {
    /// The fleet's own entries, oldest first.
    entries: Vec<Entry<'run>>,
    /// What `agentsfleetd` loaded beside them, read and never written.
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

impl<'run> Loaded<'run> {
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

    /// Reinstalls a private overlay on the replacement's current seed and grants.
    ///
    /// # Errors
    /// Bounds, duplicate keys, inconsistent custody or revoked publication refuse recovery.
    pub fn recover(
        seed: Seed<'run>,
        saved: MemoryRecovery,
        scrub: &af_secrets::Scrub,
    ) -> Result<Self> {
        saved.validate()?;
        if saved
            .superseded
            .windows(2)
            .any(|pair| pair.first() >= pair.get(1))
        {
            return Err(error::recovery_invalid());
        }
        let mut memory = Self::new(seed);
        for key in saved.superseded {
            let key = scrub.text(&key).into_owned();
            if !memory.superseded.insert(key.clone()) {
                return Err(error::recovery_invalid());
            }
            memory.remove(&key);
        }
        for delta in saved.pending {
            let delta = MemoryDelta {
                key: scrub.text(&delta.key).into_owned().into(),
                content: scrub.text(&delta.content).into_owned().into(),
                category: scrub.text(&delta.category).into_owned().into(),
                visibility: delta.visibility,
            };
            delta.validate()?;
            if !memory.superseded.contains(delta.key.as_ref())
                || memory
                    .entries
                    .iter()
                    .any(|entry| entry.delta.key == delta.key)
            {
                return Err(error::recovery_invalid());
            }
            if delta.visibility.is_workspace() && !seed.publish {
                return Err(error::not_granted());
            }
            memory.pending_bytes += delta.bytes();
            memory.entries.push(Entry {
                delta,
                pending: true,
            });
        }
        if memory.pending_bytes > MAX_PUSH_BYTES {
            return Err(error::full(memory.pending_bytes));
        }
        memory.misses.store(saved.misses, Ordering::Relaxed);
        Ok(memory)
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
            .build([query])?;
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
                {
                    let (error_code, event, message) = (
                        &code,
                        &EVENT_MISS_FAILED,
                        format_args!(
                            "a recall past the window got no answer; the run answers from its window"
                        ),
                    );
                    tracing::warn!(message, error_code, event);
                };
                None
            }
        }
    }
}

/// What the four memory tools read and write, and what the supervisor pushes.
///
/// Bound to one fleet's namespace before the run starts, so every write
/// answers for that fleet alone; another fleet's shared entries are read,
/// never written.
impl Loaded<'_> {
    /// Stores `entry`, replacing what its key held.
    ///
    /// # Errors
    /// The entry breaks a bound `af_wire` declares on a stored entry, asks
    /// the workspace to read it from a fleet that may not publish, or would
    /// no longer fit one push.
    pub fn store(&mut self, entry: MemoryDelta<'static>) -> Result<()> {
        entry.validate()?;
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

    /// The entries whose key or content holds `query`, ignoring ASCII case:
    /// key matches first, then content matches, each newest first, the
    /// fleet's own ahead of the workspace's shared ones, at most `limit`; an
    /// empty query holds in every entry. The model reads what comes back and
    /// decides what is relevant: a substring match is the ceiling on search
    /// (`docs/architecture/direction.md`).
    ///
    /// # Errors
    /// The query cannot be searched for.
    pub async fn recall<'m>(&'m self, query: &str, limit: usize) -> Result<Vec<Recalled<'m>>> {
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

    /// Every entry, or every entry in `category`, newest first: the fleet's
    /// own, then the workspace's shared ones.
    #[must_use]
    pub fn list<'m>(&'m self, category: Option<&str>) -> Vec<Recalled<'m>> {
        self.newest_first()
            .filter(|found| category.is_none_or(|wanted| found.category == wanted))
            .collect()
    }

    /// Forgets this fleet's entry under `key`; another fleet's shared entry
    /// under the same key is never touched.
    pub fn forget(&mut self, key: &str) -> Forgotten {
        // A key past the wire bound was never stored, because the daemon
        // refuses one, so there is no durable copy to hide and nothing to hold.
        if key.len() > MAX_KEY_LEN {
            return Forgotten::Unknown;
        }
        // Recorded even when the window held nothing under the key: the
        // durable copy can sit past the window, and the run asked it gone.
        self.superseded.insert(key.to_owned());
        self.remove(key)
            .map_or(Forgotten::Unknown, |_| Forgotten::ForThisRun)
    }

    /// The entries the supervisor still has to push, fenced, before the
    /// report.
    #[must_use]
    pub fn into_pending(self) -> Vec<MemoryDelta<'static>> {
        self.entries
            .into_iter()
            .filter(|entry| entry.pending)
            .map(|entry| entry.delta.into_owned())
            .collect()
    }

    /// The entries the final push would carry, as they stand: what a mid-run
    /// checkpoint writes back while the run keeps its memory.
    #[must_use]
    pub fn pending(&self) -> Vec<MemoryDelta<'_>> {
        self.entries
            .iter()
            .filter(|entry| entry.pending)
            .map(|entry| entry.delta.view())
            .collect()
    }

    /// Captures the complete local overlay without granting any saved
    /// permission.
    ///
    /// # Errors
    /// The overlay exceeds its declared bounds.
    pub fn snapshot(&self) -> Result<MemoryRecovery> {
        let mut superseded: Vec<_> = self.superseded.iter().cloned().collect();
        superseded.sort_unstable();
        let saved = MemoryRecovery::new(
            self.pending()
                .into_iter()
                .map(MemoryDelta::into_owned)
                .collect(),
            superseded,
            self.misses.load(Ordering::Relaxed).min(RECALL_MISS_CAP),
        );
        saved.validate()?;
        Ok(saved)
    }
}
