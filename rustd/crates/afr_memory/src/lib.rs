//! One run's memory: what the fleet remembered when its lease began, and what
//! the run stores, recalls and forgets before the supervisor pushes it.
//!
//! Hydrated entries are borrowed from the hydrate reply, never copied. An entry
//! the model stores is moved in from its call, and the entries the push carries
//! are moved out when the run ends ([`Memory::into_stored`]).
//!
//! The daemon only upserts a pushed entry, so a forget holds for this run: the
//! entry leaves the store and the push, and the fleet's durable copy stays
//! until a later store under its key overwrites it.

pub mod error;

use std::borrow::Cow;

use afd_wire::memory::{MAX_PUSH_BYTES, MemoryDelta};
use garde::Validate as _;

pub use self::error::{Error, Result};

/// One remembered entry, and whether this run stored it.
#[derive(Debug)]
struct Entry<'run> {
    delta: MemoryDelta<'run>,
    stored: bool,
}

/// A lease's memory, oldest entry first.
#[derive(Debug, Default)]
pub struct Memory<'run> {
    entries: Vec<Entry<'run>>,
}

impl<'run> Memory<'run> {
    /// The memory a run begins with, borrowing the hydrated `window`, which the
    /// daemon sends newest first.
    #[must_use]
    pub fn hydrated(window: &'run [MemoryDelta<'run>]) -> Self {
        let entries = window
            .iter()
            .rev()
            .map(|delta| Entry {
                delta: borrowed(delta),
                stored: false,
            })
            .collect();
        Self { entries }
    }

    /// Stores `delta`, replacing any entry under its key, for this run and the
    /// push.
    ///
    /// # Errors
    /// The entry breaks a bound the daemon declares, or the entries this run
    /// stored would no longer fit one push.
    pub fn store(&mut self, delta: MemoryDelta<'run>) -> Result<()> {
        delta.validate().map_err(error::malformed)?;
        let needed = self
            .stored()
            .filter(|kept| kept.key != delta.key)
            .map(MemoryDelta::bytes)
            .sum::<usize>()
            + delta.bytes();
        if needed > MAX_PUSH_BYTES {
            return Err(error::full(needed));
        }
        self.forget(&delta.key);
        self.entries.push(Entry {
            delta,
            stored: true,
        });
        Ok(())
    }

    /// Forgets `key` for the rest of the run; whether it was remembered.
    pub fn forget(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.delta.key != key);
        self.entries.len() != before
    }

    /// The entries whose key holds `query`, ignoring case, newest first, at
    /// most `limit`; an empty query holds in every key.
    ///
    /// The model reads what comes back and decides what is relevant: a
    /// substring of the key is the ceiling on search
    /// (`docs/architecture/direction.md`), the same filter the daemon's
    /// `ILIKE` applies to the stored rows.
    pub fn recall<'m>(
        &'m self,
        query: &str,
        limit: usize,
    ) -> impl Iterator<Item = &'m MemoryDelta<'run>> + 'm {
        let query = query.to_lowercase();
        self.newest_first()
            .filter(move |delta| delta.key.to_lowercase().contains(&query))
            .take(limit)
    }

    /// Every entry, or every entry in `category`, newest first.
    pub fn list<'m>(
        &'m self,
        category: Option<&'m str>,
    ) -> impl Iterator<Item = &'m MemoryDelta<'run>> + 'm {
        self.newest_first()
            .filter(move |delta| category.is_none_or(|wanted| delta.category == wanted))
    }

    /// The entries this run stored, moved out for the push, oldest first.
    #[must_use]
    pub fn into_stored(self) -> Vec<MemoryDelta<'static>> {
        self.entries
            .into_iter()
            .filter(|entry| entry.stored)
            .map(|entry| owned(entry.delta))
            .collect()
    }

    fn stored(&self) -> impl Iterator<Item = &MemoryDelta<'run>> {
        self.entries
            .iter()
            .filter(|entry| entry.stored)
            .map(|entry| &entry.delta)
    }

    fn newest_first(&self) -> impl Iterator<Item = &MemoryDelta<'run>> {
        self.entries.iter().rev().map(|entry| &entry.delta)
    }
}

/// `delta` as a view over the reply that holds it.
fn borrowed<'run>(delta: &'run MemoryDelta<'run>) -> MemoryDelta<'run> {
    MemoryDelta {
        key: Cow::Borrowed(&delta.key),
        content: Cow::Borrowed(&delta.content),
        category: Cow::Borrowed(&delta.category),
    }
}

/// `delta` detached from the run. A stored entry already owns its text, so
/// this moves it.
fn owned(delta: MemoryDelta<'_>) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(delta.key.into_owned()),
        content: Cow::Owned(delta.content.into_owned()),
        category: Cow::Owned(delta.category.into_owned()),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
