//! Starting a child: its tools are its parent's narrowed, and none that
//! starts a child at the depth cap; the run's caps and a pause decide
//! whether it starts; and the start is queued for the root.

use std::sync::atomic::Ordering;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::pause::Ask;
use super::{Agents, Narrow, Parent};
use crate::tool::ErrorCode;

/// A child reserved and not yet started, as the root receives it.
#[derive(Debug)]
pub(super) struct Start<S> {
    pub(super) id: u64,
    pub(super) depth: u8,
    pub(super) task: String,
    pub(super) stop: CancellationToken,
    /// The tools the child holds: its parent's, narrowed.
    pub(super) tools: S,
    /// What its parent sends it, read on its next step.
    pub(super) inbox: mpsc::UnboundedReceiver<String>,
}

/// What the root's [`super::Children`] reads: each child asked for, and
/// each ask for a pause.
#[derive(Debug)]
pub struct Requests<S> {
    pub(super) starts: mpsc::UnboundedReceiver<Start<S>>,
    pub(super) pauses: mpsc::UnboundedReceiver<Ask>,
}

impl<S> Requests<S> {
    /// Whether no child waits to start.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }
}

/// Why a child was not started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Its parent holds no tool by this name, so it may not either.
    NotHeld(String),
    /// As many children run, or were started, as the run's caps allow, or
    /// the run's children are paused.
    Capped,
}

impl Refused {
    /// The code the model reads with the refusal.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NotHeld(_) => ErrorCode::CHILD_TOOL_NOT_HELD,
            Self::Capped => ErrorCode::CHILD_CAP_REACHED,
        }
    }
}

impl<S: Narrow> Agents<S> {
    /// Reserves a child of `parent` over `task`, holding the tools of its
    /// parent's that `names` names, or all of them when it names none, and
    /// asks the root to start it. A child at the depth cap holds none that
    /// starts or steers a child.
    ///
    /// # Errors
    /// The parent holds no tool by one of `names`; or as many children run,
    /// or were started, as the run's caps allow; or the children are paused.
    pub fn start(
        &self,
        parent: Parent<'_, S>,
        task: String,
        names: Option<&[String]>,
    ) -> Result<u64, Refused> {
        let depth = parent.depth.saturating_add(1);
        let tools = names
            .map_or_else(
                || Ok(parent.tools.clone()),
                |names| parent.tools.narrowed(names),
            )
            .map_err(|missing| Refused::NotHeld(missing.to_owned()))?;
        let tools = if depth >= self.caps.depth {
            tools.without_agents()
        } else {
            tools
        };
        self.reserve(depth, task, tools, parent.stop)
    }

    /// Books a child at `depth` holding `tools`, ending with `parent`, and
    /// queues its start, unless the caps or a pause refuse it.
    fn reserve(
        &self,
        depth: u8,
        task: String,
        tools: S,
        parent: &CancellationToken,
    ) -> Result<u64, Refused> {
        let mut book = self.book();
        if self.gate.load(Ordering::Acquire) || !book.admits(self.caps) {
            return Err(Refused::Capped);
        }
        let start = book.reserve(depth, task, tools, parent);
        let id = start.id;
        // Queued under the same lock, so a pause granted after it sees the
        // start waiting. The root holds the receiver for as long as any loop
        // of the run runs; a start queued after the run ended has no loop
        // left to serve it and goes with the run.
        let _after_the_run = self.starts.send(start);
        drop(book);
        Ok(id)
    }
}
