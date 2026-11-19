//! A child as this run raises it: the run's loop over a narrowed selection,
//! its end read as the status its parent sees, and its start and end logged
//! as a pair.

use afr_tools::Selection;
use guffbolt::agents::{Agents, ChildFuture, Kind, Raise, Seat, Status};

use crate::harness::shared::Shared;
use crate::harness::{Ending, Harness};

pub(super) const EVENT_CHILD_STARTED: &str = "child_started";
pub(super) const EVENT_CHILD_ENDED: &str = "child_ended";

/// Every loop of the run shares this, so a child is the run's own loop:
/// the root's provider, sandbox, memory, ledger and meter, over a task and a
/// selection of its own.
impl<'run> Raise for Shared<'run> {
    type Tools = Selection<'run>;

    fn agents(&self) -> &Agents<Selection<'run>> {
        &self.registry
    }

    /// The run's loop over `seat`'s task, holding `tools`, run to its end.
    /// The root polls it.
    fn child<'s>(&'s self, tools: Selection<'run>, seat: Seat<'s>) -> ChildFuture<'s> {
        Box::pin(async move {
            let mut child = Harness::child(self, &tools, seat);
            let ending = child.drive().await;
            drop(child);
            status(self, ending)
        })
    }

    fn started(&self, id: u64, depth: u8) {
        let run_id = self.run_id;
        let child_id = id;
        let event = EVENT_CHILD_STARTED;
        tracing::info!(run_id, child_id, depth, event);
    }

    fn ended(&self, id: u64, depth: u8, status: Kind, calls: u64) {
        let run_id = self.run_id;
        let child_id = id;
        let status = status.as_str();
        let event = EVENT_CHILD_ENDED;
        tracing::info!(run_id, child_id, depth, status, calls, event);
    }
}

/// How a child's turns ended, as the status its parent reads: its answer
/// scrubbed like anything the model wrote.
fn status(shared: &Shared<'_>, ending: Ending) -> Status {
    match ending {
        Ending::Answered(text) => Status::Done(shared.scrub.text(&text).into_owned()),
        Ending::Failed(failure) => Status::Failed(failure.detail()),
        // A child asks its parent, never the person: a question it waits on
        // ends it the way a stop does.
        Ending::Stopped | Ending::Waiting(_) => Status::Interrupted,
    }
}
