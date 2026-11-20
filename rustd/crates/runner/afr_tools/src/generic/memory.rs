//! The fleet's memory as `guffbolt`'s memory store: the run's [`Loaded`]
//! memory, locked for one read or write, never across a call.
//!
//! The translation is one to one, so the memory tools read what they read
//! when they called the memory themselves: a store without a category
//! keeps `af_wire`'s pinned one, visibility rides as the model named it, a
//! recall answers the writer of another fleet's shared entry, and a refusal
//! keeps its code and its detail. The supervisor reads what is pending, and
//! the recovery overlay, through the same lock.

use std::borrow::Cow;

use af_wire::memory::{MemoryDelta, PINNED_CATEGORY, RECALL_LIMIT_MAX, Visibility};
use afr_memory::{Forgotten, Loaded, MemoryRecovery, Recalled};
use guffbolt::tools::code;
use guffbolt::tools::memory::{
    CORE_CATEGORY, Forgot, Memory, MemoryFuture, MemoryRefused, MemoryStore,
    RECALL_LIMIT_MAX as HARNESS_RECALL_LIMIT_MAX, Reach, Remembered,
};
use tokio::sync::Mutex;

// A recall is capped by the harness's figure, which must be the wire's. The
// default category needs no such check: a store under the harness's default
// is kept under the wire's pinned one by name, below.
const _: () = assert!(HARNESS_RECALL_LIMIT_MAX == RECALL_LIMIT_MAX);

/// One run's memory: what the memory tools are built over, and what the
/// supervisor pushes before the report.
#[derive(Debug)]
pub struct RunMemory<'run> {
    loaded: Mutex<Loaded<'run>>,
}

impl<'run> RunMemory<'run> {
    /// The run's memory, beginning as `loaded`.
    #[must_use]
    pub fn new(loaded: Loaded<'run>) -> Self {
        Self {
            loaded: Mutex::new(loaded),
        }
    }

    /// The entries the final push would carry, as they stand: what a mid-run
    /// checkpoint writes back while the run keeps its memory.
    pub async fn pending(&self) -> Vec<MemoryDelta<'static>> {
        let loaded = self.loaded.lock().await;
        loaded
            .pending()
            .into_iter()
            .map(MemoryDelta::into_owned)
            .collect()
    }

    /// The complete local overlay, for a recovery boundary.
    ///
    /// # Errors
    /// The overlay exceeds its declared bounds.
    pub async fn snapshot(&self) -> afr_memory::Result<MemoryRecovery> {
        self.loaded.lock().await.snapshot()
    }

    /// The entries the supervisor still has to push, fenced, before the
    /// report.
    #[must_use]
    pub fn into_pending(self) -> Vec<MemoryDelta<'static>> {
        self.loaded.into_inner().into_pending()
    }
}

/// A call the memory refused, with the code the model reads: a store past
/// the push is `memory_full`, a share the fleet may not make is
/// `shared_memory_not_granted`; anything else the model can correct.
fn refused(failure: &afr_memory::Error) -> MemoryRefused {
    let code = if failure.is_full() {
        code::MEMORY_FULL
    } else if failure.is_not_granted() {
        code::SHARED_MEMORY_NOT_GRANTED
    } else {
        code::INVALID_ARGUMENTS
    };
    MemoryRefused {
        code,
        detail: failure.detail(),
    }
}

/// `found` as a tool reads it back.
fn remembered(found: &Recalled<'_>) -> Remembered {
    Remembered {
        key: found.key.to_string(),
        content: found.content.to_string(),
        category: found.category.to_string(),
        writer: found.writer.as_ref().map(ToString::to_string),
    }
}

/// `memory` as the wire stores it.
fn delta(memory: Memory) -> MemoryDelta<'static> {
    let category = if memory.category == CORE_CATEGORY {
        Cow::Borrowed(PINNED_CATEGORY)
    } else {
        Cow::Owned(memory.category)
    };
    MemoryDelta {
        key: Cow::Owned(memory.key),
        content: Cow::Owned(memory.content),
        category,
        visibility: match memory.reach {
            Reach::Private => Visibility::Fleet,
            Reach::Shared => Visibility::Workspace,
        },
    }
}

impl MemoryStore for RunMemory<'_> {
    fn store(&self, memory: Memory) -> MemoryFuture<'_, ()> {
        Box::pin(async move {
            let stored = self.loaded.lock().await.store(delta(memory));
            stored.map_err(|failure| refused(&failure))
        })
    }

    fn recall<'a>(&'a self, query: &'a str, limit: usize) -> MemoryFuture<'a, Vec<Remembered>> {
        Box::pin(async move {
            let loaded = self.loaded.lock().await;
            match loaded.recall(query, limit).await {
                Ok(recalled) => Ok(recalled.iter().map(remembered).collect()),
                Err(failure) => Err(refused(&failure)),
            }
        })
    }

    fn list<'a>(&'a self, category: Option<&'a str>) -> MemoryFuture<'a, Vec<Remembered>> {
        Box::pin(async move {
            let loaded = self.loaded.lock().await;
            Ok(loaded.list(category).iter().map(remembered).collect())
        })
    }

    fn forget<'a>(&'a self, key: &'a str) -> MemoryFuture<'a, Forgot> {
        Box::pin(async move {
            let forgotten = self.loaded.lock().await.forget(key);
            Ok(match forgotten {
                Forgotten::ForThisRun => Forgot::ForThisRun,
                Forgotten::Unknown => Forgot::Unknown,
            })
        })
    }
}
