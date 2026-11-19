//! A run's children, proven against a test embedder: tools held by name, and
//! children that answer their task at once, wait on their stop, or never
//! answer.

mod book;
mod poll;

use std::future::{pending, ready};
use std::sync::{Mutex, MutexGuard, PoisonError};

use tokio_util::sync::CancellationToken;

use super::{Agents, Caps, ChildFuture, Kind, Narrow, Parent, Raise, Seat, Status};

/// The caps the cases run under: depth 2, two running, three started.
const CAPS: Caps = Caps {
    depth: 2,
    running: 2,
    started: 3,
};
/// The prefix of a tool that starts or steers a child.
const AGENT: &str = "agent_";
/// The task of a child that never answers.
const HANG: &str = "hang";
/// The task of a child that ends when it is stopped.
const STOPS: &str = "wait for the stop";

/// A loop's tools, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Names(Vec<&'static str>);

impl Narrow for Names {
    fn narrowed<'n, N: AsRef<str>>(&self, names: &'n [N]) -> Result<Self, &'n str> {
        let mut kept = Vec::with_capacity(names.len());
        for name in names.iter().map(AsRef::as_ref) {
            let held = self.0.iter().find(|held| **held == name).ok_or(name)?;
            kept.push(*held);
        }
        Ok(Self(kept))
    }

    fn without_agents(&self) -> Self {
        let kept = self
            .0
            .iter()
            .copied()
            .filter(|name| !name.starts_with(AGENT));
        Self(kept.collect())
    }
}

/// The root's tools: one that starts a child, and one that does not.
fn held() -> Names {
    Names(vec!["agent_spawn", "read"])
}

/// The root, at depth 0, stopped by `stop`, holding `tools`.
const fn root<'a>(stop: &'a CancellationToken, tools: &'a Names) -> Parent<'a, Names> {
    Parent {
        depth: 0,
        stop,
        tools,
    }
}

/// What the test embedder heard of its children.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Heard {
    /// A child started: its id and depth.
    Started(u64, u8),
    /// A child ended: its id, status and calls.
    Ended(u64, Kind, u64),
}

/// A run whose children answer with their task, unless the task says
/// otherwise.
#[derive(Debug)]
struct Run {
    agents: Agents<Names>,
    heard: Mutex<Vec<Heard>>,
}

impl Run {
    fn new(agents: Agents<Names>) -> Self {
        Self {
            agents,
            heard: Mutex::default(),
        }
    }

    fn heard(&self) -> MutexGuard<'_, Vec<Heard>> {
        self.heard.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Raise for Run {
    type Tools = Names;

    fn agents(&self) -> &Agents<Names> {
        &self.agents
    }

    fn child<'s>(&'s self, _tools: Names, seat: Seat<'s>) -> ChildFuture<'s> {
        if seat.task == HANG {
            Box::pin(pending())
        } else if seat.task == STOPS {
            Box::pin(async move {
                seat.stop.cancelled().await;
                Status::Interrupted
            })
        } else {
            Box::pin(ready(Status::Done(seat.task)))
        }
    }

    fn started(&self, id: u64, depth: u8) {
        self.heard().push(Heard::Started(id, depth));
    }

    fn ended(&self, id: u64, _depth: u8, status: Kind, calls: u64) {
        self.heard().push(Heard::Ended(id, status, calls));
    }
}
