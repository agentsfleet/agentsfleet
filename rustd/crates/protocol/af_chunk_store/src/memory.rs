//! A chunk store held in the process, for a test or a lane that has no bucket
//! (`test-util`). It counts every chunk read, so a caller can prove a refusal
//! came before any.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::durable_task::{CheckpointEntry, CheckpointFile};
use zeroize::Zeroizing;

use crate::error::{Result, refused};
use crate::store::{ChunkSource, ChunkStore, Publication, Stored};
use crate::{Change, ChunkKey, Resolved};

/// Why a name or an object is not found.
const NOT_NAMED: &str = "the store names no tree under that identifier";
const NOT_STORED: &str = "the store holds no such object";
/// Why a publication's name is refused.
const TAKEN: &str = "the store already names a tree under that identifier";
/// Why a chunk is refused.
const ORPHAN_CHUNK: &str = "a chunk does not follow its file's entry in order";

/// Every tree it names and every object they hold, in memory.
#[derive(Debug, Default)]
pub struct MemoryStore {
    held: Mutex<Held>,
    reads: AtomicU64,
}

#[derive(Debug, Default)]
struct Held {
    named: HashMap<Uuid7, Stored>,
    objects: HashMap<String, Zeroizing<Vec<u8>>>,
    generations: u64,
}

impl MemoryStore {
    /// How many objects it has read back.
    #[must_use]
    pub fn reads(&self) -> u64 {
        self.reads.load(Ordering::SeqCst)
    }

    fn held(&self) -> MutexGuard<'_, Held> {
        // Nothing panics while the lock is held, so a poisoned one still holds
        // a whole value.
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait::async_trait]
impl ChunkSource for MemoryStore {
    async fn tree(&self, id: &Uuid7) -> Result<Stored> {
        self.held()
            .named
            .get(id)
            .cloned()
            .ok_or_else(|| refused(NOT_NAMED))
    }

    async fn read(&self, object: &str) -> Result<Zeroizing<Vec<u8>>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.held()
            .objects
            .get(object)
            .cloned()
            .ok_or_else(|| refused(NOT_STORED))
    }
}

#[async_trait::async_trait]
impl ChunkStore for MemoryStore {
    async fn publish(
        &self,
        id: &Uuid7,
        parent: Option<&Uuid7>,
    ) -> Result<Box<dyn Publication + '_>> {
        let held = self.held();
        if held.named.contains_key(id) {
            return Err(refused(TAKEN));
        }
        let tree = match parent {
            Some(parent) => held
                .named
                .get(parent)
                .map(|stored| stored.tree.clone())
                .ok_or_else(|| refused(NOT_NAMED))?,
            None => Resolved::default(),
        };
        Ok(Box::new(Publishing {
            store: self,
            id: id.clone(),
            tree,
            file: None,
            objects: Vec::new(),
        }))
    }

    async fn forget(&self, id: &Uuid7) -> Result<()> {
        self.held()
            .named
            .remove(id)
            .map(drop)
            .ok_or_else(|| refused(NOT_NAMED))
    }
}

/// A publication into a [`MemoryStore`]: the tree as its parent left it, the
/// file whose chunks are arriving, and the objects to store at the commit.
struct Publishing<'a> {
    store: &'a MemoryStore,
    id: Uuid7,
    tree: Resolved,
    file: Option<CheckpointFile>,
    objects: Vec<(String, Zeroizing<Vec<u8>>)>,
}

impl Publishing<'_> {
    /// Applies the file whose chunks have all arrived.
    fn settle(&mut self) -> Result<()> {
        match self.file.take() {
            Some(file) => self.tree.apply(CheckpointEntry::File(file)),
            None => Ok(()),
        }
    }
}

#[async_trait::async_trait]
impl Publication for Publishing<'_> {
    async fn add(&mut self, change: Change) -> Result<()> {
        match change {
            Change::Entry(CheckpointEntry::File(mut file)) => {
                self.settle()?;
                file.objects.clear();
                self.file = Some(file);
                Ok(())
            }
            Change::Entry(entry) => {
                self.settle()?;
                self.tree.apply(entry)
            }
            Change::Chunk {
                key: ChunkKey::File { path, index },
                bytes,
            } => {
                let file = self
                    .file
                    .as_mut()
                    .filter(|file| {
                        file.path == path && u64::try_from(file.objects.len()) == Ok(index)
                    })
                    .ok_or_else(|| refused(ORPHAN_CHUNK))?;
                let object = Entropy::new().uuid7(af_core::clock::now())?.to_string();
                file.objects.push(object.clone());
                self.objects.push((object, bytes));
                Ok(())
            }
        }
    }

    async fn commit(mut self: Box<Self>) -> Result<u64> {
        self.settle()?;
        let mut held = self.store.held();
        held.generations += 1;
        let generation = held.generations;
        held.objects.extend(self.objects.drain(..));
        held.named.insert(
            self.id.clone(),
            Stored {
                generation,
                tree: std::mem::take(&mut self.tree),
            },
        );
        Ok(generation)
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
