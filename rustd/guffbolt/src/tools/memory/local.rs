//! The memory of a standalone harness: one process's map, gone when it ends.
//!
//! Recall is a substring match on the key and the content, ignoring ASCII
//! case, key matches first, each newest first: the model reads what comes
//! back and decides what is relevant. An embedder that keeps memory across
//! runs saves [`LocalMemory::entries`] and seeds the next run with
//! [`LocalMemory::seeded`].

use std::sync::{Mutex, MutexGuard, PoisonError};

use super::{Forgot, Memory, MemoryFuture, MemoryStore, Remembered};

/// One process's memories, oldest first.
#[derive(Debug, Default)]
pub struct LocalMemory {
    entries: Mutex<Vec<Memory>>,
}

impl LocalMemory {
    /// A memory holding `entries`, oldest first.
    #[must_use]
    pub fn seeded(entries: Vec<Memory>) -> Self {
        Self {
            entries: Mutex::new(entries),
        }
    }

    /// Every memory, oldest first.
    #[must_use]
    pub fn entries(&self) -> Vec<Memory> {
        self.held().clone()
    }

    /// The entries. Every change is one push or removal, so a panicking
    /// holder leaves them whole.
    fn held(&self) -> MutexGuard<'_, Vec<Memory>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `memory` as a recall reads it back: this run's own, so it names no writer.
fn remembered(memory: &Memory) -> Remembered {
    Remembered {
        key: memory.key.clone(),
        content: memory.content.clone(),
        category: memory.category.clone(),
        writer: None,
    }
}

impl MemoryStore for LocalMemory {
    fn store(&self, memory: Memory) -> MemoryFuture<'_, ()> {
        let mut entries = self.held();
        entries.retain(|held| held.key != memory.key);
        // A local memory has one reader, so a shared entry is kept like any.
        entries.push(memory);
        drop(entries);
        Box::pin(std::future::ready(Ok(())))
    }

    fn recall<'a>(&'a self, query: &'a str, limit: usize) -> MemoryFuture<'a, Vec<Remembered>> {
        let query = query.to_ascii_lowercase();
        let entries = self.held();
        let holds = |text: &str| text.to_ascii_lowercase().contains(&query);
        let by_key = entries.iter().rev().filter(|memory| holds(&memory.key));
        let by_content = entries
            .iter()
            .rev()
            .filter(|memory| !holds(&memory.key) && holds(&memory.content));
        let found = by_key
            .chain(by_content)
            .take(limit)
            .map(remembered)
            .collect();
        drop(entries);
        Box::pin(std::future::ready(Ok(found)))
    }

    fn list<'a>(&'a self, category: Option<&'a str>) -> MemoryFuture<'a, Vec<Remembered>> {
        let listed = self
            .held()
            .iter()
            .rev()
            .filter(|memory| category.is_none_or(|category| memory.category == category))
            .map(remembered)
            .collect();
        Box::pin(std::future::ready(Ok(listed)))
    }

    fn forget<'a>(&'a self, key: &'a str) -> MemoryFuture<'a, Forgot> {
        let mut entries = self.held();
        let before = entries.len();
        entries.retain(|memory| memory.key != key);
        let forgot = if entries.len() < before {
            Forgot::ForThisRun
        } else {
            Forgot::Unknown
        };
        drop(entries);
        Box::pin(std::future::ready(Ok(forgot)))
    }
}
