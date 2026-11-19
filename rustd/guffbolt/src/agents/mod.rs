//! A run's children: loops that a tool call starts beside the loop that made
//! it, each with its own transcript and a narrower set of its parent's tools.
//!
//! ```text
//!  a loop's call ─► Agents::start ─► reserved: an id, an inbox, a stop under its parent's
//!                                      └─ queued for the root
//!  the root ─► Children::beside(raise, the root's loop)
//!                the root goes first; whenever it waits, the set starts each
//!                child queued (Raise::child), polls every child, and answers
//!                each ask for a pause
//!  a child's loop ends ─► its status, once: done, failed or interrupted
//!  Children::end_all ─► every child still running is dropped, and ends interrupted
//! ```
//!
//! A child is the embedder's own loop, run by [`Raise::child`] over its own
//! transcript, holding its parent's tools narrowed ([`Narrow`]). Children are
//! polled beside the root in the root's own future, never spawned onto the
//! runtime, so a run that ends takes every child with it. Each child ends
//! once, and one whose future is dropped ends [`Status::Interrupted`]; a child
//! that ends takes every child it started with it.
//!
//! [`Agents`] keeps every child's state, inbox and stop, and refuses a child
//! past the run's [`Caps`]. The tools that start and steer children are
//! ordinary tools of the embedder's, the way Codex's `multi_agents_v2` tools
//! are: each calls [`Agents`] and answers with [`Spawned`], [`State`],
//! [`Accepted`], a list of [`Listed`], [`Agents::delegated`] or
//! [`not_found`].
//!
//! A [`Pause`] proves the children quiet, for an embedder that saves a run:
//! while one is held no child starts, and [`Agents::census`] reads every
//! child for [`Agents::restored`] to bring back.

use std::pin::Pin;

use serde::{Serialize, Serializer};
use tokio_util::sync::CancellationToken;

pub use self::answer::{Accepted, Listed, Spawned, State, not_found};
pub use self::children::Children;
pub use self::pause::{Census, Counted, Pause};
pub use self::registry::{Agents, Tether};
pub use self::start::{Refused, Requests};

mod answer;
mod children;
mod pause;
mod registry;
mod start;

/// A child's loop, from its first turn to its end: the status it ended on.
pub type ChildFuture<'s> = Pin<Box<dyn Future<Output = Status> + Send + 's>>;

/// How far one run's children may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// How deep a child may be: the root is 0, and a child this deep holds
    /// none of the tools that start children.
    pub depth: u8,
    /// How many children may run at once.
    pub running: usize,
    /// How many children one run may start.
    pub started: u64,
}

/// Where a child is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// It runs.
    Running,
    /// It ended on its answer.
    Done(String),
    /// It ended on a failure of its own, with its detail.
    Failed(String),
    /// It ended before its answer: interrupted, or with its parent.
    Interrupted,
}

impl Status {
    /// Whether the child has ended.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        !matches!(self, Self::Running)
    }

    /// The status, by name.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::Running => Kind::Running,
            Self::Done(_) => Kind::Done,
            Self::Failed(_) => Kind::Failed,
            Self::Interrupted => Kind::Interrupted,
        }
    }
}

/// A status by name, as the model and a log read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// It runs.
    Running,
    /// It ended on its answer.
    Done,
    /// It ended on a failure of its own.
    Failed,
    /// It ended before its answer.
    Interrupted,
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
        }
    }
}

impl Serialize for Kind {
    fn serialize<Z: Serializer>(&self, serializer: Z) -> Result<Z::Ok, Z::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The tools a loop holds, as a child of it may hold them: never more than
/// its parent's.
pub trait Narrow: Clone + Send {
    /// These tools narrowed to `names`.
    ///
    /// # Errors
    /// The first of `names` these tools do not hold; nothing is narrowed.
    fn narrowed<'n, N: AsRef<str>>(&self, names: &'n [N]) -> Result<Self, &'n str>;

    /// These tools without any that starts or steers a child: what a child
    /// at the depth cap holds.
    #[must_use]
    fn without_agents(&self) -> Self;
}

/// The loop a child is asked of: how deep it is, its stop, and its tools.
#[derive(Debug)]
pub struct Parent<'a, S> {
    /// How many loops it is inside: 0 for the root.
    pub depth: u8,
    /// Cancelled when it is to end; its children's stops descend from it.
    pub stop: &'a CancellationToken,
    /// The tools it holds, which its child's narrow.
    pub tools: &'a S,
}

// By hand, because a derive would ask `S: Copy` of a view that only borrows
// its tools.
impl<S> Clone for Parent<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Parent<'_, S> {}

/// What a child's loop is given as it starts.
#[derive(Debug)]
pub struct Seat<'s> {
    /// How many loops the child is inside: one more than its parent.
    pub depth: u8,
    /// What the child is asked to do: its first message.
    pub task: String,
    /// Cancelled to end the child. It descends from its parent's, so the
    /// child ends with its parent.
    pub stop: CancellationToken,
    /// What the child keeps of its place in the run.
    pub tether: Tether<'s>,
}

/// The embedder's half of a run's children: the loop each one runs, and what
/// hears each one start and end.
///
/// [`Children`] calls these as each child starts and ends, and waits on none
/// of them. The embedder is shared with every child's loop, so it is `Sync`.
pub trait Raise: Sync {
    /// The tools a child holds.
    type Tools: Narrow;

    /// The run's children.
    fn agents(&self) -> &Agents<Self::Tools>;

    /// A child's own loop, given `seat` and holding `tools`, run to its end:
    /// the status it ended on. Called once per child as it starts, before
    /// its future is first polled.
    fn child<'s>(&'s self, tools: Self::Tools, seat: Seat<'s>) -> ChildFuture<'s>;

    /// Child `id`, at `depth`, starts.
    fn started(&self, _id: u64, _depth: u8) {}

    /// Child `id`, at `depth`, ended as `status` after `calls` calls that
    /// answered. Each child that started ends here once: on its own, or
    /// [`Kind::Interrupted`] when its future is dropped first.
    fn ended(&self, _id: u64, _depth: u8, _status: Kind, _calls: u64) {}
}

#[cfg(test)]
mod tests;
