//! A child's turns as the registry keeps them: one begun, one ended, and
//! one queued again for a follow-up its turn never read.

use super::{Agents, Child, Turn};
use crate::agents::Status;
use crate::drive::DriveState;
use crate::transcript::Message;

impl Agents {
    /// The turn child `id` was queued for, as it starts: its stop, and a
    /// copy of what it keeps, which stays in the book until the turn ends,
    /// numbering its steps after every step an earlier turn began. `None`
    /// when there is no such turn, or when the child was interrupted or
    /// closed, or the loop ended, before it started; that turn ends here.
    pub(in crate::agents) fn begin(&self, id: u64) -> Option<Turn> {
        let mut book = self.book();
        let child = book.children.get_mut(&id)?;
        if *child.status.borrow() != Status::Running || child.begun {
            return None;
        }
        let stop = child.turn.clone()?;
        if stop.is_cancelled() {
            drop(book);
            self.ended(id, Status::Interrupted, None);
            return None;
        }
        child.begun = true;
        let mut state = child.state.clone();
        state.numbered_after(child.steps);
        Some(Turn {
            depth: child.depth,
            stop,
            tools: child.tools.clone(),
            transcript: child.transcript.clone(),
            state,
        })
    }

    /// Queues another turn for child `id`, which is running or about to:
    /// a fresh stop under its life, sent under the book's lock, so a pause
    /// granted after it sees the turn waiting. The root holds the receiver
    /// for as long as any loop of the run runs; a turn queued after the run
    /// ended has no loop left to serve it and goes with the run.
    pub(super) fn queue(&self, id: u64, child: &mut Child) {
        child.turn = Some(child.life.child_token());
        child.begun = false;
        let _after_the_run = self.0.turns.send(id);
    }

    /// Records that child `id`'s turn ended as `status`, keeping `kept`, its
    /// transcript and state after the turn, when the turn ran to its end. A
    /// child closed meanwhile ends closed; one owed a turn for a follow-up
    /// it never read runs again at once, still running, and reads it.
    pub(in crate::agents) fn ended(
        &self,
        id: u64,
        status: Status,
        kept: Option<(Vec<Message>, DriveState)>,
    ) {
        let mut book = self.book();
        let Some(child) = book.children.get_mut(&id) else {
            return;
        };
        if child.status.borrow().is_final() {
            return;
        }
        child.turn = None;
        child.begun = false;
        if let Some((transcript, state)) = kept {
            child.transcript = transcript;
            child.state = state;
        }
        let closing = child.closing;
        if std::mem::take(&mut child.owed) && !closing && !child.life.is_cancelled() {
            self.queue(id, child);
            return;
        }
        child.status.send_replace(status);
        book.running = book.running.saturating_sub(1);
        if closing {
            book.close(id);
        }
    }

    /// What child `id` was sent since its last step, oldest first; a
    /// follow-up read here is owed no turn of its own.
    pub(in crate::agents) fn take_inbox(&self, id: u64) -> Vec<String> {
        self.book()
            .children
            .get_mut(&id)
            .map_or_else(Vec::new, |child| {
                child.owed = false;
                std::mem::take(&mut child.inbox)
            })
    }

    /// Counts one call child `id` made that answered.
    pub(in crate::agents) fn called(&self, id: u64) {
        if let Some(child) = self.book().children.get_mut(&id) {
            child.calls += 1;
        }
    }

    /// Notes that child `id`'s turn lent a call at step `step`, so a later
    /// turn numbers its steps after it even when this one is dropped.
    pub(in crate::agents) fn lent(&self, id: u64, step: u64) {
        if let Some(child) = self.book().children.get_mut(&id) {
            child.steps = child.steps.max(step);
        }
    }
}
