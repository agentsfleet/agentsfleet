//! A run's children as a pause saves them, and as a later run restores
//! them.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::{Agents, Book, Child, Requests, Shared};
use crate::agents::{Caps, Census, Counted, Status};
use crate::drive::DriveState;

impl Agents {
    /// The children `state` saved, under `caps`, and where the root reads
    /// each turn to start.
    ///
    /// Every child `state` holds was idle or closed when it was saved, as
    /// one saved under a [`super::Pause`] is: each comes back as it was,
    /// its transcript with it, and the ids and counts go on from where they
    /// were. A child saved running, which no pause allows, comes back
    /// interrupted.
    #[must_use]
    pub fn restored(caps: Caps, state: &DriveState) -> (Self, Requests) {
        let census = state.children();
        let root = CancellationToken::new();
        let mut book = Book {
            next: census.next,
            started: census.started,
            ..Book::default()
        };
        for counted in &census.children {
            let life = book
                .children
                .get(&counted.parent)
                .map_or(&root, |parent| &parent.life)
                .child_token();
            let status = match &counted.status {
                Status::Running => Status::Interrupted,
                other => other.clone(),
            };
            if status != Status::Closed {
                book.open += 1;
            }
            book.children
                .insert(counted.id, child_of(counted, status, life));
        }
        let (turns, turn_requests) = mpsc::unbounded_channel();
        let (pauses, pause_requests) = mpsc::unbounded_channel();
        let shared = Shared {
            book: Mutex::new(book),
            caps,
            turns,
            pauses,
            gate: Arc::new(AtomicBool::new(false)),
            root,
        };
        let requests = Requests {
            turns: turn_requests,
            pauses: pause_requests,
        };
        (Self(Arc::new(shared)), requests)
    }

    /// Every child as it is now, oldest first, with the run's counts.
    pub(in crate::agents) fn census_now(&self) -> Census {
        let book = self.book();
        let children = book
            .children
            .iter()
            .map(|(id, child)| Counted {
                id: *id,
                parent: child.parent,
                depth: child.depth,
                calls: child.calls,
                tools: child.tools.clone(),
                status: child.status.borrow().clone(),
                transcript: child.transcript.clone(),
                state: child.state.clone(),
            })
            .collect();
        Census {
            next: book.next,
            started: book.started,
            children,
        }
    }
}

/// The child `counted` saves, as `status`, its life `life`.
fn child_of(counted: &Counted, status: Status, life: CancellationToken) -> Child {
    let (status, _watched) = watch::channel(status);
    Child {
        parent: counted.parent,
        depth: counted.depth,
        tools: counted.tools.clone(),
        calls: counted.calls,
        status,
        inbox: Vec::new(),
        life,
        turn: None,
        transcript: counted.transcript.clone(),
        state: counted.state.clone(),
        closing: false,
    }
}
