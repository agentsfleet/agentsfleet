//! Moving a workspace's memory from one store to another without losing a write.
//!
//! ```text
//!   settled(from) ──► flipping(from → to) ──copy every entry──► settled(to)
//!                            │                    │
//!                            │              copy fails
//!                            ▼                    ▼
//!                writes land in to, then from   settled(from), as it was
//! ```
//!
//! While the copy runs, reads stay on `from` and every write lands in both, so
//! no write lands only in the store being left. The copy never replaces a newer
//! row — a write that reached `to` mid-copy is newer than the snapshot row the
//! copy carries — so no write is lost to it either. No endpoint calls this: the
//! first vendor store brings the caller with it.

use std::sync::Arc;

use afd_core::id::Uuid7;

use crate::error::{Error, Result, moving};
use crate::memories::Memories;
use crate::route::Route;
use crate::store::MemoryStore;

/// A flip began copying.
const EVENT_STARTED: &str = "memory_flip_started";
/// A flip copied every entry and switched.
const EVENT_COMPLETED: &str = "memory_flip_completed";
/// A flip failed and the workspace stayed where it was.
const EVENT_FAILED: &str = "memory_flip_failed";

/// What a completed flip did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flipped {
    /// Entries copied into the new store.
    pub copied: usize,
}

impl Memories {
    /// Copies every entry of `workspace`'s fleets into `to`, writer and
    /// visibility kept, then makes `to` the workspace's store.
    ///
    /// # Errors
    /// Refuses a workspace already flipping, and reports a copy `to` would not
    /// take — after which `workspace` is on the store it was on, which holds
    /// every entry it held and every write made meanwhile.
    pub async fn flip(&self, workspace: &Uuid7, to: Arc<dyn MemoryStore>) -> Result<Flipped> {
        let settled = self.routes().of(workspace);
        if let Some(mirror) = &settled.mirror {
            return Err(moving(mirror.name()));
        }
        let from = Arc::clone(&settled.store);
        let copying = Arc::new(Route {
            store: Arc::clone(&from),
            mirror: Some(Arc::clone(&to)),
        });
        if !self.routes().swap(workspace, &settled, &copying) {
            return Err(moving(to.name()));
        }
        let (workspace_id, from_store, to_store) = (workspace.as_str(), from.name(), to.name());
        tracing::info!(
            workspace_id,
            from_store,
            to_store,
            event = EVENT_STARTED,
            "a workspace's memory began copying to another store"
        );
        let copied = match copy(from.as_ref(), to.as_ref(), workspace).await {
            Ok(copied) => copied,
            Err(refused) => {
                // Back to the very route the flip replaced, so a write that
                // loaded it before the flip sees no change and is done.
                self.routes().swap(workspace, &copying, &settled);
                return Err(failed(workspace, &from, &to, refused));
            }
        };
        let switched = Arc::new(Route::settled(Arc::clone(&to)));
        if !self.routes().swap(workspace, &copying, &switched) {
            return Err(failed(workspace, &from, &to, moving(to_store)));
        }
        tracing::info!(
            workspace_id,
            from_store,
            to_store,
            copied,
            event = EVENT_COMPLETED,
            "a workspace's memory moved to another store"
        );
        Ok(Flipped { copied })
    }
}

/// Every entry of `workspace` in `from`, imported into `to` one at a time.
async fn copy(from: &dyn MemoryStore, to: &dyn MemoryStore, workspace: &Uuid7) -> Result<usize> {
    let entries = from.export(workspace).await?;
    for entry in &entries {
        to.import(workspace, entry).await?;
    }
    Ok(entries.len())
}

/// Logs a flip that left `workspace` where it was, and answers `refused`.
fn failed(
    workspace: &Uuid7,
    from: &Arc<dyn MemoryStore>,
    to: &Arc<dyn MemoryStore>,
    refused: Error,
) -> Error {
    let (workspace_id, from_store, to_store) = (workspace.as_str(), from.name(), to.name());
    let error_code = refused.code().as_str();
    tracing::warn!(
        workspace_id,
        from_store,
        to_store,
        error_code,
        event = EVENT_FAILED,
        "a workspace's memory could not move; it stays on its store"
    );
    refused
}

#[cfg(all(test, feature = "test-util"))]
#[path = "flip_tests.rs"]
mod tests;
