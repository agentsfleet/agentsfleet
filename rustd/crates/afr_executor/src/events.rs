//! One process's events between the link that hears them and the caller that
//! reads them.
//!
//! The link must never wait on a caller: every process shares its connection,
//! and a session the model reads once a minute would stall the rest. So the
//! link feeds a process's output into a bounded store of what is unread
//! (`edges`) and goes on; the caller reads it at its own pace, and output it
//! fell too far behind on is dropped from the middle and counted. Codex keeps
//! the same bound on its client (`exec-server/src/client.rs`, a 1 MiB retained
//! history per process).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use tokio::sync::Notify;
use tokio::sync::mpsc::error::TryRecvError;

use crate::api::{Ending, ProcessEvent, Stream};
use crate::edges::{Chunk, EDGE_BYTES, Next, Unread};

/// What both ends share.
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    /// Woken on every change; one permit is kept when no one waits, so a
    /// change between a reader's look and its wait is never missed.
    changed: Notify,
}

#[derive(Debug)]
struct State {
    unread: Unread,
    /// How the process ended, until the reader takes it.
    ending: Option<Ending>,
    /// Whether output was left behind when it ended.
    output_abandoned: bool,
    /// Whether anything more can arrive: not once the ending was fed or the
    /// feed went away.
    finished: bool,
}

impl Shared {
    /// The state. Nothing panics while holding it, and a poisoned one is
    /// still whole, so poisoning is passed over.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The reading end of a process's events: its output in order, a count where
/// output was dropped unread, then exactly one [`ProcessEvent::Ended`].
#[derive(Debug)]
pub struct Events {
    shared: Arc<Shared>,
}

/// The feeding end. Dropping it without [`Feed::end`] finishes the events
/// with no ending.
#[derive(Debug)]
pub struct Feed {
    shared: Arc<Shared>,
}

impl Events {
    /// A new pair: the feed the link (or a stand-in executor) writes, and
    /// the events a caller reads.
    #[must_use]
    pub fn channel() -> (Feed, Self) {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                unread: Unread::new(EDGE_BYTES),
                ending: None,
                output_abandoned: false,
                finished: false,
            }),
            changed: Notify::new(),
        });
        let feed = Feed {
            shared: Arc::clone(&shared),
        };
        (feed, Self { shared })
    }

    /// The next event, waiting for one; `None` once the ending was read, or
    /// the feed went away without one.
    pub async fn recv(&mut self) -> Option<ProcessEvent> {
        loop {
            match self.try_recv() {
                Ok(event) => return Some(event),
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => self.shared.changed.notified().await,
            }
        }
    }

    /// The next event when one is waiting.
    ///
    /// # Errors
    /// [`TryRecvError::Empty`] while the process may say more,
    /// [`TryRecvError::Disconnected`] once nothing more will arrive.
    pub fn try_recv(&mut self) -> Result<ProcessEvent, TryRecvError> {
        let mut state = self.shared.state();
        if let Some(next) = state.unread.pop() {
            return Ok(match next {
                Next::Output(Chunk { stream, data }) => ProcessEvent::Output { stream, data },
                Next::Omitted(bytes) => ProcessEvent::Omitted { bytes },
            });
        }
        match state.ending.take() {
            Some(ending) => Ok(ProcessEvent::Ended {
                ending,
                output_abandoned: state.output_abandoned,
            }),
            None if state.finished => Err(TryRecvError::Disconnected),
            None => Err(TryRecvError::Empty),
        }
    }

    /// Whether anything more can arrive: once the process ended, or its
    /// connection did, what is left is only to be read.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.shared.state().finished
    }

    /// Waits, reading nothing, until nothing more can arrive. What arrives
    /// meanwhile waits in the bounded store, so a caller that reads only at
    /// the end holds a process's edges, never all it said.
    pub async fn finished(&self) {
        while !self.is_finished() {
            self.shared.changed.notified().await;
        }
    }
}

impl Feed {
    /// Hands on one chunk of output.
    pub fn output(&self, stream: Stream, data: Bytes) {
        self.shared.state().unread.push(Chunk { stream, data });
        self.shared.changed.notify_one();
    }

    /// Hands on how the process ended and whether its output was still open
    /// when the drain gave it up, the last thing it says. Dropping the feed
    /// then finishes the events and wakes the reader.
    pub fn end(self, ending: Ending, output_abandoned: bool) {
        let mut state = self.shared.state();
        state.ending = Some(ending);
        state.output_abandoned = output_abandoned;
    }
}

/// The events are finished only here: a live feed can always say more.
impl Drop for Feed {
    fn drop(&mut self) {
        self.shared.state().finished = true;
        self.shared.changed.notify_one();
    }
}

#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;
