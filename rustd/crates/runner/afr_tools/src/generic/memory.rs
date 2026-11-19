//! The fleet's memory as `guffbolt`'s memory store: the backend the fleet is
//! bound to, held for one read or write, never across a call.
//!
//! The translation is one to one, so the memory tools read what they read
//! when they called the backend themselves: a store without a category
//! keeps `af_wire`'s pinned one, visibility rides as the model named it, a
//! recall answers the writer of another fleet's shared entry, and a refusal
//! keeps its code and its detail.

use std::borrow::Cow;

use af_wire::memory::{MemoryDelta, PINNED_CATEGORY, RECALL_LIMIT_MAX, Visibility};
use afr_memory::{Forgotten, MemoryBackend, Recalled};
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

/// One lease's memory, lent to a call.
#[derive(Debug)]
pub(super) struct Leased<'l, 'run> {
    backend: &'l Mutex<Box<dyn MemoryBackend + 'run>>,
}

impl<'l, 'run> Leased<'l, 'run> {
    pub(super) const fn new(backend: &'l Mutex<Box<dyn MemoryBackend + 'run>>) -> Self {
        Self { backend }
    }
}

/// A call the backend refused, with the code the model reads: a store past
/// the push is `memory_full`, a share the fleet may not make is
/// `workspace_memory_not_granted`; anything else the model can correct.
fn refused(failure: &afr_memory::Error) -> MemoryRefused {
    let code = if failure.is_full() {
        code::MEMORY_FULL
    } else if failure.is_not_granted() {
        code::WORKSPACE_MEMORY_NOT_GRANTED
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

impl MemoryStore for Leased<'_, '_> {
    fn store(&self, memory: Memory) -> MemoryFuture<'_, ()> {
        Box::pin(async move {
            let category = if memory.category == CORE_CATEGORY {
                Cow::Borrowed(PINNED_CATEGORY)
            } else {
                Cow::Owned(memory.category)
            };
            let delta = MemoryDelta {
                key: Cow::Owned(memory.key),
                content: Cow::Owned(memory.content),
                category,
                visibility: match memory.reach {
                    Reach::Fleet => Visibility::Fleet,
                    Reach::Workspace => Visibility::Workspace,
                },
            };
            let stored = self.backend.lock().await.store(delta).await;
            stored.map_err(|failure| refused(&failure))
        })
    }

    fn recall<'a>(&'a self, query: &'a str, limit: usize) -> MemoryFuture<'a, Vec<Remembered>> {
        Box::pin(async move {
            let backend = self.backend.lock().await;
            match backend.recall(query, limit).await {
                Ok(recalled) => Ok(recalled.iter().map(remembered).collect()),
                Err(failure) => Err(refused(&failure)),
            }
        })
    }

    fn list<'a>(&'a self, category: Option<&'a str>) -> MemoryFuture<'a, Vec<Remembered>> {
        Box::pin(async move {
            let backend = self.backend.lock().await;
            match backend.list(category).await {
                Ok(listed) => Ok(listed.iter().map(remembered).collect()),
                Err(failure) => Err(refused(&failure)),
            }
        })
    }

    fn forget<'a>(&'a self, key: &'a str) -> MemoryFuture<'a, Forgot> {
        Box::pin(async move {
            let forgotten = self.backend.lock().await.forget(key).await;
            match forgotten {
                Ok(Forgotten::ForThisRun) => Ok(Forgot::ForThisRun),
                Ok(Forgotten::Unknown) => Ok(Forgot::Unknown),
                Err(failure) => Err(refused(&failure)),
            }
        })
    }
}
