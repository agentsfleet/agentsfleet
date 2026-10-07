//! A child's loop, from its first turn to its end, and the guard that ends
//! it exactly once.

use tokio::sync::mpsc;

use super::registry::{Registry, Start, Status};
use crate::harness::shared::Shared;
use crate::harness::{Ending, Harness};

pub(super) const EVENT_CHILD_STARTED: &str = "child_started";
pub(super) const EVENT_CHILD_ENDED: &str = "child_ended";

/// Runs the child `start` describes to its end, under `guard`. The root
/// loop polls it.
pub(crate) async fn run<'s, 'run: 's>(
    shared: &'s Shared<'run>,
    start: Start<'run>,
    guard: Guard<'s, 'run>,
) {
    let selection = start.selection;
    let mut child = Harness::child(shared, &selection, start.seat, guard);
    let ending = child.drive().await;
    child.conclude(ending);
}

impl Harness<'_, '_> {
    /// Ends the child this loop is with how its turns ended; the root never
    /// concludes, it finishes.
    pub(crate) fn conclude(self, ending: Ending) {
        let Some(Tether { guard, .. }) = self.child else {
            return;
        };
        let status = match ending {
            Ending::Answered(text) => Status::Done(self.shared.scrub.text(&text).into_owned()),
            Ending::Failed(failure) => Status::Failed(failure.detail()),
            Ending::Stopped => Status::Interrupted,
        };
        guard.ended(status);
    }
}

/// What a child loop holds that the root does not: the guard that ends it
/// once, and the inbox its parent's `send_input` writes to.
pub(crate) struct Tether<'s, 'run> {
    pub(crate) guard: Guard<'s, 'run>,
    pub(crate) input: mpsc::UnboundedReceiver<String>,
}

/// One child's start and end, logged as a pair on every path: its end is
/// recorded when its loop concludes, and `interrupted` when its future is
/// dropped before that, with its parent.
pub(crate) struct Guard<'s, 'run> {
    registry: &'s Registry<'run>,
    lease_id: &'s str,
    id: u64,
    depth: u8,
    armed: bool,
}

impl<'s, 'run> Guard<'s, 'run> {
    /// Starts child `id` at `depth` in the log and holds its end.
    pub(crate) fn new(shared: &'s Shared<'run>, id: u64, depth: u8) -> Self {
        let lease_id = shared.lease_id;
        let child_id = id;
        let event = EVENT_CHILD_STARTED;
        tracing::info!(lease_id, child_id, depth, event);
        Self {
            registry: &shared.registry,
            lease_id,
            id,
            depth,
            armed: true,
        }
    }

    /// Counts one call the child made.
    pub(crate) fn called(&self) {
        self.registry.called(self.id);
    }

    /// Ends the child as `status`.
    fn ended(mut self, status: Status) {
        self.armed = false;
        self.end(status);
    }

    fn end(&self, status: Status) {
        let status = self.registry.ended(self.id, status).as_str();
        let calls = self.registry.calls(self.id);
        let lease_id = self.lease_id;
        let child_id = self.id;
        let depth = self.depth;
        let event = EVENT_CHILD_ENDED;
        tracing::info!(lease_id, child_id, depth, status, calls, event);
    }
}

impl Drop for Guard<'_, '_> {
    fn drop(&mut self) {
        if self.armed {
            self.end(Status::Interrupted);
        }
    }
}
