//! The children a run started, polled beside the root loop.
//!
//! A child's loop borrows what the run shares, so it cannot be handed to the
//! runtime as a task of its own; it is a future the root polls whenever its
//! own work waits, on a step, a call or a checkpoint. A loop anywhere in the
//! run asks for a child through the registry, and the root starts it here, so
//! a child's children are polled the same way. Ending the set drops every
//! future, and each child and each call it held open end interrupted on the
//! way down.

use std::fmt;
use std::pin::Pin;

use futures_util::StreamExt as _;
use futures_util::stream::FuturesUnordered;
use tokio::sync::oneshot;

use super::start::Start;
use super::{Agents, ChildFuture, Pause, Raise, Requests, Seat, Status};

/// One child's loop, from its first step to its guard's end.
type Running<'s> = Pin<Box<dyn Future<Output = ()> + Send + 's>>;

/// Every child a run started and has not ended, `S` being the tools a child
/// holds.
pub struct Children<'s, S> {
    running: FuturesUnordered<Running<'s>>,
    requests: Requests<S>,
    /// Each ask for a pause that waits for the set to be quiet.
    waiting: Vec<oneshot::Sender<Option<Pause>>>,
}

impl<'s, S> Children<'s, S> {
    /// No child yet; `requests` is where the registry asks for one.
    #[must_use]
    pub fn new(requests: Requests<S>) -> Self {
        Self {
            running: FuturesUnordered::new(),
            requests,
            waiting: Vec::new(),
        }
    }

    /// Drives `driven` to its end, starting each child asked for through
    /// `raise` and polling every child whenever `driven` waits. `driven` goes
    /// first, so a loop that never waits is never slowed by its children.
    pub async fn beside<R, F>(&mut self, raise: &'s R, driven: F) -> F::Output
    where
        R: Raise<Tools = S>,
        F: Future,
    {
        tokio::pin!(driven);
        loop {
            self.answer_quiet(raise.agents());
            let asked = tokio::select! {
                biased;
                out = &mut driven => return out,
                Some(start) = self.requests.starts.recv() => Some(start),
                Some(()) = self.running.next(), if !self.running.is_empty() => None,
                Some(ask) = self.requests.pauses.recv() => {
                    if ask.waits {
                        self.waiting.push(ask.reply);
                    } else {
                        let pause = raise.agents().try_pause(||
                            self.running.is_empty() && self.requests.is_empty());
                        let _receiver_gone = ask.reply.send(pause);
                    }
                    None
                }
            };
            if let Some(start) = asked {
                self.running.push(raised(raise, start));
            }
        }
    }

    /// Drops every child still running, ends each one queued and never
    /// started, and answers every ask for a pause still waiting with none.
    /// Answers a pause over the quiet set, when one may be had.
    pub fn end_all(&mut self, agents: &Agents<S>) -> Option<Pause> {
        self.requests.starts.close();
        self.requests.pauses.close();
        self.running.clear();
        while let Ok(start) = self.requests.starts.try_recv() {
            agents.ended(start.id, Status::Interrupted);
        }
        while let Ok(ask) = self.requests.pauses.try_recv() {
            let _receiver_gone = ask.reply.send(None);
        }
        for reply in self.waiting.drain(..) {
            let _receiver_gone = reply.send(None);
        }
        agents.try_pause(|| self.running.is_empty() && self.requests.is_empty())
    }

    /// Answers every ask waiting for the set to be quiet, once no child runs
    /// or waits to start: the first takes the pause, and each one after it
    /// is refused while that pause is held.
    fn answer_quiet(&mut self, agents: &Agents<S>) {
        if self.waiting.is_empty() || !self.running.is_empty() || !self.requests.is_empty() {
            return;
        }
        for reply in self.waiting.drain(..) {
            let _receiver_gone = reply.send(agents.try_pause(|| true));
        }
    }
}

impl<S> fmt::Debug for Children<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Children")
            .field("running", &self.running.len())
            .field("queued", &!self.requests.is_empty())
            .field("waiting", &self.waiting.len())
            .finish()
    }
}

/// The child `start` asks for, guarded from here, so a child dropped before
/// its first poll still ends interrupted.
fn raised<R: Raise>(raise: &R, start: Start<R::Tools>) -> Running<'_> {
    let Start {
        id,
        depth,
        task,
        stop,
        tools,
        inbox,
    } = start;
    let guard = Guard::new(raise, id, depth);
    let tether = raise.agents().tether(id, inbox);
    let seat = Seat {
        depth,
        task,
        stop,
        tether,
    };
    let child = raise.child(tools, seat);
    Box::pin(run(guard, child))
}

/// Runs `child` to its end under `guard`. The child's future goes before the
/// guard on every path, so whatever it holds open, such as a call or a child
/// of its own, ends before it does.
async fn run<R: Raise>(guard: Guard<'_, R>, child: ChildFuture<'_>) {
    let status = child.await;
    guard.ended(status);
}

/// One child's start and end, heard as a pair on every path: its end is
/// recorded when its loop ends, and as interrupted when its future is dropped
/// before that, with its parent.
struct Guard<'s, R: Raise> {
    raise: &'s R,
    id: u64,
    depth: u8,
    armed: bool,
}

impl<'s, R: Raise> Guard<'s, R> {
    /// Starts child `id` at `depth`, and holds its end.
    fn new(raise: &'s R, id: u64, depth: u8) -> Self {
        raise.started(id, depth);
        Self {
            raise,
            id,
            depth,
            armed: true,
        }
    }

    /// Ends the child as `status`.
    fn ended(mut self, status: Status) {
        self.armed = false;
        self.end(status);
    }

    fn end(&self, status: Status) {
        let agents = self.raise.agents();
        let status = agents.ended(self.id, status);
        let calls = agents.calls(self.id);
        self.raise.ended(self.id, self.depth, status, calls);
    }
}

impl<R: Raise> Drop for Guard<'_, R> {
    fn drop(&mut self) {
        if self.armed {
            self.end(Status::Interrupted);
        }
    }
}
