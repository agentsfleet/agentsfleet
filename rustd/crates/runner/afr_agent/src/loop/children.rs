//! The children a run started, polled beside the root loop.
//!
//! A child's loop borrows the run's shared state, so it cannot be handed to
//! the runtime as a task of its own; it is a future the root loop polls
//! whenever its own work waits, on a turn, a call or a checkpoint. A loop
//! anywhere in the run asks for a child through the registry's request
//! channel, and the root starts it here, so a child's children are polled
//! the same way. Ending the set drops every future, and each child's guard
//! and open call end `interrupted` on the way down.

use std::pin::Pin;

use futures_util::StreamExt as _;
use futures_util::stream::FuturesUnordered;
use tokio::sync::mpsc;

use super::shared::Shared;
use crate::nested::{self, Guard, Start};

/// One child's loop, from its first turn to its guard's end.
type Running<'s> = Pin<Box<dyn Future<Output = ()> + Send + 's>>;

/// Every child the run has started and not ended.
pub(super) struct Children<'s, 'run> {
    running: FuturesUnordered<Running<'s>>,
    requests: mpsc::UnboundedReceiver<Start<'run>>,
}

impl<'s, 'run: 's> Children<'s, 'run> {
    /// No child yet; `requests` is where the registry asks for one.
    pub(super) fn new(requests: mpsc::UnboundedReceiver<Start<'run>>) -> Self {
        Self {
            running: FuturesUnordered::new(),
            requests,
        }
    }

    /// Drives `driven` to its end, starting each child asked for and polling
    /// every child whenever `driven` waits. `driven` goes first, so a loop
    /// that never waits is never slowed by its children.
    pub(super) async fn beside<F: Future>(
        &mut self,
        shared: &'s Shared<'run>,
        driven: F,
    ) -> F::Output {
        tokio::pin!(driven);
        loop {
            let asked = tokio::select! {
                biased;
                out = &mut driven => return out,
                Some(start) = self.requests.recv() => Some(start),
                Some(()) = self.running.next(), if !self.running.is_empty() => None,
            };
            if let Some(start) = asked {
                // Guarded from here, so a child dropped before its first poll
                // still ends `interrupted` in the log and the registry.
                let guard = Guard::new(shared, start.id, start.seat.depth);
                self.running
                    .push(Box::pin(nested::child(shared, start, guard)));
            }
        }
    }

    /// Drops every child still running.
    pub(super) fn end_all(&mut self) {
        self.running.clear();
    }
}
