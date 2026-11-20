//! A pause: proof that a run's children are quiet, and a hold that keeps
//! them so, for an embedder that saves the run between steps.
//!
//! Only the root's poller knows whether a turn's future is still unwinding
//! or a turn is queued, so a loop asks it for a pause, and the poller grants
//! one only when no turn runs, none unwinds and none waits to start. A start
//! reads the hold under the book's lock, so none slips past a pause granted
//! after it was queued. An ask that waits is answered once the set is quiet,
//! after every turn's end is recorded, while the poller keeps polling them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::{Agents, Status};
use crate::drive::DriveState;
use crate::transcript::Message;

/// Proof that a run's children were quiet when it was granted, and a hold on
/// them: no turn starts while it is kept, and dropping it lets them start
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
/// started, and each child, as the run's [`DriveState`] carries them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Census {
    /// The id the last child took; the next one takes the one after.
    pub next: u64,
    /// How many children the run started, against its cap.
    pub started: u64,
    /// Each child, oldest first.
    pub children: Vec<Counted>,
}

impl Census {
    /// Whether the run started no child.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.next == 0 && self.started == 0 && self.children.is_empty()
    }
}

/// One child, as a census counts it: where it sits, where it is, and what it
/// keeps for its next turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counted {
    /// Its id.
    pub id: u64,
    /// The loop that started it: 0 for the root.
    pub parent: u64,
    /// How many loops it is inside.
    pub depth: u8,
    /// How many calls it made that answered.
    pub calls: u64,
    /// The tools it holds, by name.
    pub tools: Vec<String>,
    /// Where it is.
    pub status: Status,
    /// Its transcript as its last turn left it; none once closed.
    pub transcript: Vec<Message>,
    /// Its loop's progress as its last turn left it.
    pub state: DriveState,
}

/// One ask for a pause, as the root's poller reads it.
#[derive(Debug)]
pub(super) struct Ask {
    /// Where the pause goes, or `None` when there is none to grant.
    pub(super) reply: oneshot::Sender<Option<Pause>>,
    /// Whether the ask waits for the set to be quiet, rather than being
    /// refused while a turn runs.
    pub(super) waits: bool,
}

impl Agents {
    /// Asks the root's poller to pause the run's children. `None` when a
    /// turn runs, unwinds or waits to start, when the children are paused
    /// already, or when the run has ended.
    pub async fn pause(&self) -> Option<Pause> {
        self.ask(false).await
    }

    /// Waits until no turn of the run runs, unwinds or waits to start, then
    /// pauses the children. The root's poller keeps polling every turn
    /// meanwhile, so each ends on its own and its end is recorded before
    /// this answers. `None` when the children are paused already or the run
    /// has ended.
    ///
    /// Only the root waits so: a child that waited would wait on itself. A
    /// turn that never ends holds this as long as the caller waits, so the
    /// caller races it against its stop.
    pub async fn quiet(&self) -> Option<Pause> {
        self.ask(true).await
    }

    /// Asks the root's poller for a pause, which `waits` for the set to be
    /// quiet or is refused while a turn runs.
    async fn ask(&self, waits: bool) -> Option<Pause> {
        let (reply, receipt) = oneshot::channel();
        self.0.pauses.send(Ask { reply, waits }).ok()?;
        receipt.await.ok().flatten()
    }

    /// The run's children as `pause` holds them; `None` when `pause` is
    /// another run's.
    #[must_use]
    pub fn census(&self, pause: &Pause) -> Option<Census> {
        self.holds(pause).then(|| self.census_now())
    }

    /// Writes the run's children, as `pause` holds them, into `state`, the
    /// root's, so a run resumed from it brings them back
    /// ([`Agents::restored`]). Whether it wrote them: not when `pause` is
    /// another run's.
    pub fn save(&self, pause: &Pause, state: &mut DriveState) -> bool {
        let Some(census) = self.census(pause) else {
            return false;
        };
        state.children = census;
        true
    }

    /// Whether `pause` is this run's, and held.
    fn holds(&self, pause: &Pause) -> bool {
        Arc::ptr_eq(&pause.gate, &self.0.gate) && self.0.gate.load(Ordering::Acquire)
    }

    /// A pause, when no turn runs and `quiet` says no turn's future is left
    /// and none waits: what only the root's poller can say.
    pub(super) fn try_pause(&self, quiet: impl FnOnce() -> bool) -> Option<Pause> {
        let book = self.book();
        if book.running != 0 || !quiet() || self.0.gate.swap(true, Ordering::AcqRel) {
            return None;
        }
        drop(book);
        Some(Pause {
            gate: Arc::clone(&self.0.gate),
        })
    }
}
