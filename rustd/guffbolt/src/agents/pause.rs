//! A pause: proof that a run's children are quiet, and a hold that keeps
//! them so, for an embedder that saves the run between turns.
//!
//! Only the root's poller knows whether a child's future is still unwinding
//! or a start is queued, so a loop asks it for a pause, and the poller grants
//! one only when no child runs, none unwinds and none waits to start. A start
//! reads the hold under the book's lock, so none slips past a pause granted
//! after it was queued.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::oneshot;

use super::{Agents, Status};

/// Proof that a run's children were quiet when it was granted, and a hold on
/// them: no child starts while it is kept, and dropping it lets them start
/// again.
#[derive(Debug)]
pub struct Pause {
    gate: Arc<AtomicBool>,
}

impl Drop for Pause {
    fn drop(&mut self) {
        self.gate.store(false, Ordering::Release);
    }
}

/// A run's children at one quiet moment: the last id given, how many
/// started, and each child.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Census {
    /// The id the last child took; the next one takes the one after.
    pub next: u64,
    /// How many children the run started, against its cap.
    pub started: u64,
    /// Each child, oldest first.
    pub children: Vec<Counted>,
}

/// One child, as a census counts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted {
    /// Its id.
    pub id: u64,
    /// How many loops it is inside.
    pub depth: u8,
    /// How many calls it made that answered.
    pub calls: u64,
    /// Where it is.
    pub status: Status,
}

impl<S> Agents<S> {
    /// Asks the root's poller to pause the run's children. `None` when a
    /// child runs, unwinds or waits to start, when the children are paused
    /// already, or when the run has ended.
    pub async fn pause(&self) -> Option<Pause> {
        let (reply, receipt) = oneshot::channel();
        self.pauses.send(reply).ok()?;
        receipt.await.ok().flatten()
    }

    /// The run's children as `pause` holds them; `None` when `pause` is
    /// another run's.
    #[must_use]
    pub fn census(&self, pause: &Pause) -> Option<Census> {
        if !Arc::ptr_eq(&pause.gate, &self.gate) || !self.gate.load(Ordering::Acquire) {
            return None;
        }
        Some(self.book().census())
    }

    /// A pause, when no child runs and `quiet` says no child's future is
    /// left and no start waits: what only the root's poller can say.
    pub(super) fn try_pause(&self, quiet: impl FnOnce() -> bool) -> Option<Pause> {
        let book = self.book();
        if book.running() != 0 || !quiet() || self.gate.swap(true, Ordering::AcqRel) {
            return None;
        }
        drop(book);
        Some(Pause {
            gate: Arc::clone(&self.gate),
        })
    }
}
