//! The children a recovered run brings back: ended ones only, with their
//! results and counters, and no loop, inbox or start of their own.
//!
//! A checkpoint is taken only under a [`Pause`], which `guffbolt` grants
//! while no child runs, unwinds or waits to start; its census is what is
//! saved here, bounded and scrubbed again on the way back.

use af_secrets::Scrub;
use afr_tools::Selection;
use garde::Validate;
use guffbolt::agents::{Agents, Census, Counted, Pause, Requests, Status};
use serde::{Deserialize, Serialize};

use super::{CAPS, CHILDREN_PER_RUN_MAX, NESTED_DEPTH_MAX};
use crate::Result;
use crate::error::recovery_invalid;

/// Where a saved child is, as a checkpoint carries it: `guffbolt`'s status,
/// each text bounded.
#[derive(Clone, Serialize, Deserialize, Validate)]
enum SavedStatus {
    Running,
    Done(#[garde(length(bytes, max = 65_536))] String),
    Failed(#[garde(length(bytes, max = 65_536))] String),
    Interrupted,
}

impl From<Status> for SavedStatus {
    fn from(status: Status) -> Self {
        match status {
            Status::Running => Self::Running,
            Status::Done(answer) => Self::Done(answer),
            Status::Failed(detail) => Self::Failed(detail),
            Status::Interrupted => Self::Interrupted,
        }
    }
}

impl From<SavedStatus> for Status {
    fn from(saved: SavedStatus) -> Self {
        match saved {
            SavedStatus::Running => Self::Running,
            SavedStatus::Done(answer) => Self::Done(answer),
            SavedStatus::Failed(detail) => Self::Failed(detail),
            SavedStatus::Interrupted => Self::Interrupted,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
struct SavedChild {
    #[garde(range(min = 1, max = CHILDREN_PER_RUN_MAX))]
    id: u64,
    #[garde(range(min = 1, max = NESTED_DEPTH_MAX))]
    depth: u8,
    #[garde(skip)]
    calls: u64,
    #[garde(dive)]
    status: SavedStatus,
}

impl From<Counted> for SavedChild {
    fn from(counted: Counted) -> Self {
        Self {
            id: counted.id,
            depth: counted.depth,
            calls: counted.calls,
            status: counted.status.into(),
        }
    }
}

impl From<SavedChild> for Counted {
    fn from(saved: SavedChild) -> Self {
        Self {
            id: saved.id,
            depth: saved.depth,
            calls: saved.calls,
            status: saved.status.into(),
        }
    }
}

/// Terminal results and counters, with no live process, inbox or permission.
#[derive(Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChildRecovery {
    #[garde(range(max = CHILDREN_PER_RUN_MAX))]
    next: u64,
    #[garde(range(max = CHILDREN_PER_RUN_MAX))]
    started: u64,
    #[garde(length(max = usize::try_from(CHILDREN_PER_RUN_MAX).unwrap_or_default()), dive)]
    children: Vec<SavedChild>,
}

impl ChildRecovery {
    /// The children of `agents` as `pause` holds them, as a checkpoint
    /// saves them.
    ///
    /// # Errors
    /// `pause` is another run's, or the children break the bounds a
    /// restore checks.
    pub(crate) fn capture(agents: &Agents<Selection<'_>>, pause: &Pause) -> Result<Self> {
        let census = agents
            .census(pause)
            .ok_or_else(|| recovery_invalid("the child capture permit belongs to another run"))?;
        let saved = Self {
            next: census.next,
            started: census.started,
            children: census.children.into_iter().map(SavedChild::from).collect(),
        };
        saved.check()?;
        Ok(saved)
    }

    /// A registry under the run's caps holding these children, each answer
    /// and detail scrubbed again, and where its root reads each new child.
    ///
    /// # Errors
    /// A child that had not ended, counters that disagree, or a bound
    /// broken, before or after the scrub.
    pub(crate) fn restored<'run>(
        mut self,
        scrub: &Scrub,
    ) -> Result<(Agents<Selection<'run>>, Requests<Selection<'run>>)> {
        self.check()?;
        for child in &mut self.children {
            if let SavedStatus::Done(text) | SavedStatus::Failed(text) = &mut child.status {
                *text = scrub.text(text).into_owned();
            }
        }
        self.check()?;
        let census = Census {
            next: self.next,
            started: self.started,
            children: self.children.into_iter().map(Counted::from).collect(),
        };
        Ok(Agents::restored(CAPS, census))
    }

    pub(crate) fn check(&self) -> Result<()> {
        self.validate()?;
        if self.next != self.started
            || self.started != self.children.len() as u64
            || self
                .children
                .iter()
                .any(|child| matches!(child.status, SavedStatus::Running) || child.id > self.next)
            || self
                .children
                .windows(2)
                .any(|pair| pair.first().map(|child| child.id) >= pair.get(1).map(|child| child.id))
        {
            return Err(recovery_invalid(
                "terminal child identifiers or counters are inconsistent",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
