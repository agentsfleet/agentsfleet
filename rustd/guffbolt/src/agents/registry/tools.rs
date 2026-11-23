//! What the seven tools ask of the registry: a wait, the list, a message,
//! a follow-up, an interrupt and a close.

use std::time::Duration;

use super::Agents;
use crate::agents::{Caps, Kind, Listed, Status};

impl Agents {
    /// Waits for child `id`'s turn to end, or for `timeout` to pass, and
    /// answers where it is; `None` for an id no child has. With no timeout
    /// it waits as long as the caller does.
    pub async fn wait(&self, id: u64, timeout: Option<Duration>) -> Option<Status> {
        let mut watched = self.book().children.get(&id)?.status.subscribe();
        let ended = match timeout {
            Some(limit) => tokio::time::timeout(limit, watched.wait_for(Status::is_final))
                .await
                .ok()
                .and_then(|ended| ended.ok().map(|status| status.clone())),
            None => watched
                .wait_for(Status::is_final)
                .await
                .ok()
                .map(|status| status.clone()),
        };
        Some(ended.unwrap_or_else(|| watched.borrow().clone()))
    }

    /// Every child, oldest first.
    #[must_use]
    pub fn list(&self) -> Vec<Listed> {
        self.book()
            .children
            .iter()
            .map(|(id, child)| Listed {
                agent_id: *id,
                status: child.status.borrow().kind(),
                depth: child.depth,
                calls: child.calls,
            })
            .collect()
    }

    /// Queues `message` for child `id`: a running child reads it on its next
    /// step, an idle one with its next turn. Whether it was taken, which a
    /// closed child never does; `None` for an id no child has.
    #[must_use]
    pub fn send(&self, id: u64, message: String) -> Option<bool> {
        let mut book = self.book();
        let child = book.children.get_mut(&id)?;
        let open = *child.status.borrow() != Status::Closed;
        if open {
            child.inbox.push(message);
        }
        drop(book);
        Some(open)
    }

    /// Hands child `id` `message` as another task: an idle child runs a turn
    /// over its transcript, a running one reads it on its next step, or in
    /// a turn of its own when the running one ends first. Whether
    /// it was taken, which a closed child, or an idle one while the children
    /// are paused, never does; `None` for an id no child has.
    #[must_use]
    pub fn follow_up(&self, id: u64, message: String) -> Option<bool> {
        let mut book = self.book();
        let paused = self.paused();
        let child = book.children.get_mut(&id)?;
        let status = child.status.borrow().clone();
        let taken = match status {
            Status::Closed => false,
            Status::Running => {
                child.inbox.push(message);
                child.owed = true;
                true
            }
            Status::Done(_) | Status::Failed(_) | Status::Interrupted if paused => false,
            Status::Done(_) | Status::Failed(_) | Status::Interrupted => {
                child.inbox.push(message);
                child.status.send_replace(Status::Running);
                self.queue(id, child);
                book.running += 1;
                true
            }
        };
        drop(book);
        Some(taken)
    }

    /// Ends child `id`'s running turn where it is, or its queued turn before
    /// it begins, and answers the status it had; the child stays open for
    /// another task. `None` for an id no child has.
    #[must_use]
    pub fn interrupt(&self, id: u64) -> Option<Kind> {
        let book = self.book();
        let child = book.children.get(&id)?;
        let before = child.status.borrow().kind();
        if let Some(turn) = &child.turn {
            turn.cancel();
        }
        drop(book);
        Some(before)
    }

    /// Closes child `id` and every open child it started, and answers the
    /// status it had. An idle child closes here; a running one closes as
    /// its turn, stopped here, ends. `None` for an id no child has.
    #[must_use]
    pub fn close(&self, id: u64) -> Option<Kind> {
        let mut book = self.book();
        let before = book.children.get(&id)?.status.borrow().kind();
        let mut closing = vec![id];
        while let Some(next) = closing.pop() {
            closing.extend(
                book.children
                    .iter()
                    .filter(|(_, child)| child.parent == next)
                    .map(|(id, _)| *id),
            );
            book.close(next);
        }
        drop(book);
        Some(before)
    }

    /// The caps this run's children are held to.
    #[must_use]
    pub fn caps(&self) -> Caps {
        self.0.caps
    }
}
