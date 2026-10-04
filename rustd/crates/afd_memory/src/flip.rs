//! Moving a workspace's memory from one store to another without losing a write,
//! and without bringing back one the fleet forgot.
//!
//! ```text
//!   settled(from) ──► flipping(from → to) ──prune, then copy──► settled(to)
//!                            │                    │
//!                            │        prune or copy fails, a write
//!                            │        missed `to`, or the flip is
//!                            │        dropped before it switches
//!                            ▼                    ▼
//!                writes land in from, then to   settled(from), as it was
//! ```
//!
//! While the flip runs, reads stay on `from` and every write lands in both:
//! `from` first, so a write reaches `to` only once `from` holds it, and a
//! write the caller is told succeeded never sits only in the store being left.
//! A write that reached `from` and then failed on `to` marks the route, and the
//! flip never switches to a store it knows missed a write: it puts the
//! workspace back on `from`, which holds that write. A write whose `to` half
//! fails while the flip switches is reported to its caller as failed, so no
//! write the caller was told succeeded is lost.
//!
//! The flip first prunes `to` of every row `from` no longer holds, so an entry
//! the fleet forgot cannot come back from a failed earlier copy or from a
//! store the workspace lived in before. The prune deletes only the version it
//! read: a row goes only while it is no newer than the `updated_at` the prune
//! saw, so a rewrite after the prune's read carries a later instant and stays,
//! even when `to` and `from` hold the same rows, as two store objects over one
//! database do. One edge remains: a rewrite stamped no later than the version
//! the prune read (in its very millisecond, or by a writer whose clock runs
//! behind) goes with it. Between distinct stores the copy brings it back; over
//! shared rows nothing does.
//!
//! Only then does the flip copy. The copy's snapshot is read after the prune,
//! so between distinct stores it is the backstop that needs no clock: a write
//! the prune deleted from `to` already sat in `from` and is carried back, and
//! a write whose `from` half lands after the snapshot lands in `to` after the
//! prune too. The copy never replaces a newer row, so no write is lost to it
//! either.
//!
//! A workspace already on `to`, recognised by identity, stays as it is with
//! nothing pruned or copied: a copy onto its own rows brings back a row
//! removed mid-copy.
//!
//! A flip that ends without switching (refused, or dropped by a caller that
//! stopped waiting) puts the workspace back on `from`: writes stop reaching
//! `to`, and the next flip or forget is taken. No endpoint calls this: the
//! first vendor store brings the caller with it.

use std::collections::HashSet;
use std::sync::Arc;

use afd_core::error_code::INTERNAL_OPERATION_FAILED;
use afd_core::id::Uuid7;

use crate::error::{Error, Result, missed_write, moving};
use crate::memories::Memories;
use crate::record::Owner;
use crate::route::{Route, Routes};
use crate::store::MemoryStore;

/// A flip began pruning and copying.
const EVENT_STARTED: &str = "memory_flip_started";
/// A flip pruned, copied every entry and switched.
const EVENT_COMPLETED: &str = "memory_flip_completed";
/// A flip ended without switching, refused or dropped, and the workspace
/// stayed where it was.
const EVENT_FAILED: &str = "memory_flip_failed";

/// What a completed flip did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flipped {
    /// Rows removed from the new store because the old one no longer held
    /// them.
    pub pruned: usize,
    /// Entries copied into the new store.
    pub copied: usize,
}

impl Memories {
    /// Removes from `to` every row of `workspace` the current store no longer
    /// holds, copies every entry of the workspace's fleets into it, writer and
    /// visibility kept, then makes `to` the workspace's store. A workspace
    /// already on `to` stays as it is, and the answer counts nothing.
    ///
    /// # Errors
    /// Refuses a workspace already flipping, reports a prune or a copy `to`
    /// would not take, and refuses to switch to `to` once a write made
    /// meanwhile failed on it — after each, `workspace` is on the store it was
    /// on, which holds every entry it held and every write made meanwhile. A
    /// flip dropped before it answers leaves the workspace there too.
    pub async fn flip(&self, workspace: &Uuid7, to: Arc<dyn MemoryStore>) -> Result<Flipped> {
        let Some(flipping) = Flipping::begin(self.routes(), workspace, &to)? else {
            return Ok(Flipped {
                pruned: 0,
                copied: 0,
            });
        };
        let from = Arc::clone(&flipping.settled.store);
        let (workspace_id, from_store, to_store) = (workspace.as_str(), from.name(), to.name());
        tracing::info!(
            workspace_id,
            from_store,
            to_store,
            event = EVENT_STARTED,
            "a workspace's memory began copying to another store"
        );
        let flipped = match fill(from.as_ref(), to.as_ref(), workspace).await {
            Ok(flipped) => flipped,
            Err(refused) => return Err(flipping.refuse(failed(workspace, &from, &to, refused))),
        };
        flipping
            .switch(&to)
            .map_err(|refused| failed(workspace, &from, &to, refused))?;
        let Flipped { pruned, copied } = flipped;
        tracing::info!(
            workspace_id,
            from_store,
            to_store,
            pruned,
            copied,
            event = EVENT_COMPLETED,
            "a workspace's memory moved to another store"
        );
        Ok(flipped)
    }
}

/// How far a flip got, which decides what its guard does when dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Pruning or copying; a guard dropped here was abandoned by its caller.
    Running,
    /// Refused, and logged as such by the flip.
    Refused,
    /// The workspace is on its new store.
    Switched,
}

/// A workspace mid-flip, its route writing both stores.
///
/// Dropped before it switches — refused, or abandoned by a caller that
/// stopped waiting — it puts the workspace back on the route the flip
/// replaced. Without it, a dropped flip would leave every write mirrored and
/// every later flip and forget refused until the process restarts.
#[derive(Debug)]
struct Flipping<'a> {
    routes: &'a Routes,
    workspace: &'a Uuid7,
    /// The route the flip installed, writing both stores.
    copying: Arc<Route>,
    /// The route it replaced, and puts back.
    settled: Arc<Route>,
    /// The store being filled, for an abandoned flip's log line.
    to_store: &'static str,
    stage: Stage,
}

impl<'a> Flipping<'a> {
    /// Installs a route over `workspace` that writes `to` as well, or answers
    /// `None` for a workspace already on `to`.
    ///
    /// # Errors
    /// Refuses a workspace already flipping, or one another flip reached
    /// first.
    fn begin(
        routes: &'a Routes,
        workspace: &'a Uuid7,
        to: &Arc<dyn MemoryStore>,
    ) -> Result<Option<Self>> {
        let settled = routes.of(workspace);
        if let Some(mirror) = &settled.mirror {
            return Err(moving(mirror.name()));
        }
        if Arc::ptr_eq(&settled.store, to) {
            return Ok(None);
        }
        let copying = Arc::new(Route::copying(Arc::clone(&settled.store), Arc::clone(to)));
        if !routes.swap(workspace, &settled, &copying) {
            return Err(moving(to.name()));
        }
        let to_store = to.name();
        let stage = Stage::Running;
        Ok(Some(Self {
            routes,
            workspace,
            copying,
            settled,
            to_store,
            stage,
        }))
    }

    /// Makes `to` the workspace's store.
    ///
    /// # Errors
    /// Refuses once a write through the flip's route failed on `to`, so the
    /// old store, which holds that write, goes back; and refuses when the
    /// route moved under the flip, where the route that replaced the flip's
    /// stays.
    fn switch(mut self, to: &Arc<dyn MemoryStore>) -> Result<()> {
        let switched = Arc::new(Route::settled(Arc::clone(to)));
        let outcome = if self.copying.missed() {
            Err(missed_write(to.name()))
        } else if self.routes.swap(self.workspace, &self.copying, &switched) {
            Ok(())
        } else {
            Err(moving(to.name()))
        };
        self.stage = if outcome.is_ok() {
            Stage::Switched
        } else {
            Stage::Refused
        };
        outcome
    }

    /// Ends the flip on `refused`, which the caller logged; the route goes
    /// back as the guard drops.
    fn refuse(mut self, refused: Error) -> Error {
        self.stage = Stage::Refused;
        refused
    }
}

impl Drop for Flipping<'_> {
    fn drop(&mut self) {
        if self.stage == Stage::Switched {
            return;
        }
        // Back to the very route the flip replaced, so a write that loaded it
        // before the flip sees no change and is done. The swap takes only
        // while the flip's own route is in, so a route that replaced it stays.
        self.routes
            .swap(self.workspace, &self.copying, &self.settled);
        // A caller that stopped waiting is an exit like a refusal: the flip
        // logged its start, so it logs its end.
        if self.stage == Stage::Running {
            let workspace_id = self.workspace.as_str();
            let from_store = self.settled.store.name();
            let to_store = self.to_store;
            let error_code = INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(
                workspace_id,
                from_store,
                to_store,
                error_code,
                event = EVENT_FAILED,
                "a flip stopped before it switched; the workspace stays on its store"
            );
        }
    }
}

/// Prunes `to` of what `from` no longer holds, then copies `from` into it.
async fn fill(from: &dyn MemoryStore, to: &dyn MemoryStore, workspace: &Uuid7) -> Result<Flipped> {
    let pruned = prune(from, to, workspace).await?;
    let copied = copy(from, to, workspace).await?;
    Ok(Flipped { pruned, copied })
}

/// Removes from `to` every row of `workspace` that `from` no longer holds,
/// answering how many it removed.
///
/// `to` is read first. A row a write put in `to` reached `from` before it, so
/// `from`'s later read holds it too and it stays. Each row goes only while it
/// is no newer than the version read here, so a write that replaced it since
/// stays even when `to` and `from` share rows; between distinct stores, the
/// copy, whose snapshot is read after this returns, is the backstop.
async fn prune(from: &dyn MemoryStore, to: &dyn MemoryStore, workspace: &Uuid7) -> Result<usize> {
    let held = to.export(workspace).await?;
    let source = from.export(workspace).await?;
    let kept: HashSet<(&Uuid7, &str)> = source
        .iter()
        .map(|row| (&row.fleet, row.key.as_str()))
        .collect();
    let mut pruned = 0;
    for row in held
        .iter()
        .filter(|row| !kept.contains(&(&row.fleet, row.key.as_str())))
    {
        let owner = Owner {
            workspace,
            fleet: &row.fleet,
        };
        let removed = to.forget_stale(owner, &row.key, row.updated_at_ms).await?;
        pruned += usize::from(removed);
    }
    Ok(pruned)
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
