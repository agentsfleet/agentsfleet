//! A run's children: loops a tool call starts beside the loop that made it,
//! each over its own transcript with its parent's tools or fewer, and driven
//! by the harness itself.
//!
//! ```text
//!  spawn_agent ─► Agents: an id, a seat under the caps, a turn queued
//!  the root ─► Children::beside(raise, the root's loop)
//!                whenever the root waits, the poller asks Raise::child for
//!                the child's conversation, seats it (its inbox, its tools,
//!                its stop) and drives it, the way drive runs the root
//!  a turn ends ─► the child goes idle: its status, transcript and state kept
//!  followup_task ─► an idle child runs again over its transcript
//!  close_agent ─► the child ends closed, and its seat is free
//!  Children::end_all ─► every running turn is dropped, every open child
//!                       ends interrupted
//! ```
//!
//! The embedder implements [`Raise`]: the conversation a child's turn runs
//! over, built from a [`Seat`]. That conversation is the embedder's, as the
//! root's is, so its model, its observer and its records serve the child the
//! way they serve the root; the harness keeps what is the child's own: its
//! transcript between turns, its inbox, its tools narrowed from its parent's
//! and its stop. Children are polled beside the root in the root's own future,
//! never spawned onto the runtime, so a run that ends takes every child with
//! it.
//!
//! The seven tools that start and steer children are the harness's own,
//! built with [`crate::tools::ToolsBuilder::with_agents`] and named as Codex
//! names its multi-agent tools. Each answers with an order the loop runs in
//! the calling loop's place, so a child's spawn is that child's own.
//!
//! A [`Pause`] proves the children quiet, for an embedder that saves a run:
//! while one is held no turn starts, and [`Agents::save`] writes every child
//! into the run's [`crate::DriveState`], from which [`Agents::restored`]
//! brings them back. [`Agents::pause`] is refused while a turn runs;
//! [`Agents::quiet`] waits for every turn to end first, as a root does before
//! its run waits on a question.

use std::fmt;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::conversation::Conversation;
use crate::transcript::Transcript;

pub use self::children::Children;
pub(crate) use self::children::quiet;
pub(crate) use self::order::Caller;
pub use self::order::{Action, Fork, Order, Refused, Spawn};
pub use self::pause::{Census, Counted, Pause};
pub use self::registry::{Agents, Listed, Requests};

mod children;
mod fork;
mod order;
mod pause;
mod registry;
mod seated;

/// What starts a child.
pub const SPAWN_AGENT: &str = "spawn_agent";
/// What queues a message for a child.
pub const SEND_INPUT: &str = "send_input";
/// What gives an idle child another task.
pub const FOLLOWUP_TASK: &str = "followup_task";
/// What waits for a child's turn to end.
pub const WAIT_AGENT: &str = "wait_agent";
/// What lists a run's children.
pub const LIST_AGENTS: &str = "list_agents";
/// What closes a child.
pub const CLOSE_AGENT: &str = "close_agent";
/// What interrupts a child's turn.
pub const INTERRUPT_AGENT: &str = "interrupt_agent";
/// The seven, which a child at the depth cap holds none of.
pub const TOOLS: [&str; 7] = [
    SPAWN_AGENT,
    SEND_INPUT,
    FOLLOWUP_TASK,
    WAIT_AGENT,
    LIST_AGENTS,
    CLOSE_AGENT,
    INTERRUPT_AGENT,
];

/// How far one run's children may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// How deep a child may be: the root is 0, and a child this deep holds
    /// none of the seven tools.
    pub depth: u8,
    /// How many children may be open at once: running, or idle with their
    /// transcript. A closed child frees its seat.
    pub open: usize,
    /// How many children one run may start.
    pub started: u64,
}

/// Where a child is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Its turn runs.
    Running,
    /// Its turn ended on its answer; it is idle.
    Done(String),
    /// Its turn ended on a failure of its own, with its detail; it is idle.
    Failed(String),
    /// Its turn ended before its answer: interrupted, or with the loop; it is
    /// idle.
    Interrupted,
    /// It was closed, and keeps nothing.
    Closed,
}

impl Status {
    /// Whether its turn has ended: idle or closed.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        !matches!(self, Self::Running)
    }

    /// Whether it takes another turn: open, and not running one.
    #[must_use]
    pub const fn is_idle(&self) -> bool {
        !matches!(self, Self::Running | Self::Closed)
    }

    /// The status, by name.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::Running => Kind::Running,
            Self::Done(_) => Kind::Done,
            Self::Failed(_) => Kind::Failed,
            Self::Interrupted => Kind::Interrupted,
            Self::Closed => Kind::Closed,
        }
    }
}

/// A status by name, as the model and a log read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Its turn runs.
    Running,
    /// Its turn ended on its answer.
    Done,
    /// Its turn ended on a failure of its own.
    Failed,
    /// Its turn ended before its answer.
    Interrupted,
    /// It was closed.
    Closed,
}

impl Kind {
    /// The spelling the model and a log read.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Closed => "closed",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which loop of a run is acting: the root, or a child, and how deep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Me {
    /// 0 for the root.
    pub(crate) id: u64,
    /// How many loops it is inside: 0 for the root.
    pub(crate) depth: u8,
}

impl Me {
    /// The run's root.
    pub(crate) const ROOT: Self = Self { id: 0, depth: 0 };
}

/// What a child's conversation is built from as one of its turns starts.
#[derive(Debug)]
pub struct Seat {
    /// The child's id, which every call about it names.
    pub id: u64,
    /// How many loops it is inside: one more than its parent.
    pub depth: u8,
    /// Cancelled to end the turn: when the child is interrupted or closed,
    /// or the loop ends. The conversation's [`Conversation::stop`] answers
    /// it; the harness races the turn's model calls and tool calls against
    /// it already.
    pub stop: CancellationToken,
    /// The child's transcript so far: the parent's turns it was forked from,
    /// and every earlier turn of its own. This turn's task arrives as its
    /// input.
    pub transcript: Transcript,
}

/// The embedder's half of a run's children: the conversation each child's
/// turn runs over.
///
/// The harness calls [`Raise::child`] as each turn starts and drives the
/// conversation it answers with its own transcript and tools; the embedder
/// hears the turn through that conversation's observer, as it hears the
/// root. The embedder is shared with every child's turn, so it is `Sync`.
pub trait Raise: Sync {
    /// A child's conversation. Its failure reads, in its alternate form
    /// (`{:#}`), as the detail its parent is told.
    type Child<'s>: Conversation<Failure: fmt::Display>
    where
        Self: 's;

    /// The run's children.
    fn agents(&self) -> &Agents;

    /// The conversation child `seat.id`'s turn runs over: the embedder's
    /// model, observer, request and tools, holding `seat.transcript` and
    /// answering `seat.stop`. The harness offers the child only the tools
    /// its parent gave it, and hands it its input.
    fn child(&self, seat: Seat) -> Self::Child<'_>;
}

#[cfg(test)]
mod tests;
