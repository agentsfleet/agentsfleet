//! Which store each workspace's memory lives in, read without a lock.
//!
//! ```text
//!   settled    reads ─► store          writes ─► store
//!   flipping   reads ─► store (from)   writes ─► store (from), then mirror (to)
//! ```
//!
//! A flipping write reaches the store being left first, so it reaches the
//! store being filled only once the one being left holds it.
//!
//! The table is one `HashMap` behind an [`ArcSwap`]: every call loads it
//! without blocking, and only a flip — rare, and refused while another is
//! running for the same workspace — replaces it, by compare-and-swap. A writer
//! that saw an older route catches up instead of waiting: see
//! [`crate::Memories`]'s write path.

use std::collections::HashMap;
use std::sync::Arc;

use afd_core::id::Uuid7;
use arc_swap::{ArcSwap, Guard};

use crate::store::MemoryStore;

/// Where one workspace's memory lives right now.
#[derive(Debug)]
pub(crate) struct Route {
    /// The store every read answers from, and the first one a write reaches.
    pub(crate) store: Arc<dyn MemoryStore>,
    /// During a flip, the store being filled. Written after `store`, so a
    /// write reaches it only once the store being left holds that write, and
    /// a write the caller is told succeeded never sits only in the store
    /// being left.
    pub(crate) mirror: Option<Arc<dyn MemoryStore>>,
}

impl Route {
    /// One store answering everything.
    pub(crate) const fn settled(store: Arc<dyn MemoryStore>) -> Self {
        Self {
            store,
            mirror: None,
        }
    }

    /// Every store a write must reach, in the order it reaches them: the
    /// route's own store, then a flip's mirror.
    pub(crate) fn writers(&self) -> impl Iterator<Item = &Arc<dyn MemoryStore>> {
        std::iter::once(&self.store).chain(self.mirror.iter())
    }
}

/// Every workspace's route.
#[derive(Debug)]
pub(crate) struct Routes {
    /// The route a workspace no flip has touched takes.
    default: Arc<Route>,
    /// The workspaces a flip has touched.
    table: ArcSwap<HashMap<Uuid7, Arc<Route>>>,
}

impl Routes {
    /// Every workspace on `store` until a flip says otherwise.
    pub(crate) fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self {
            default: Arc::new(Route::settled(store)),
            table: ArcSwap::from_pointee(HashMap::new()),
        }
    }

    /// `workspace`'s route as it stands.
    pub(crate) fn of(&self, workspace: &Uuid7) -> Arc<Route> {
        Arc::clone(self.table.load().get(workspace).unwrap_or(&self.default))
    }

    /// Replaces `workspace`'s route with `next`, but only while it is still
    /// `current`; whether it was.
    pub(crate) fn swap(&self, workspace: &Uuid7, current: &Arc<Route>, next: &Arc<Route>) -> bool {
        let mut seen = self.table.load_full();
        loop {
            let installed = seen.get(workspace).unwrap_or(&self.default);
            if !Arc::ptr_eq(installed, current) {
                return false;
            }
            let mut replaced = HashMap::clone(&seen);
            replaced.insert(workspace.clone(), Arc::clone(next));
            let previous = self.table.compare_and_swap(&seen, Arc::new(replaced));
            if Arc::ptr_eq(&previous, &seen) {
                return true;
            }
            seen = Guard::into_inner(previous);
        }
    }
}
