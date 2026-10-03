//! What a run begins with, and how it asks for more.

use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;

use afd_wire::memory::{MemoryDelta, MemoryRecallResponse, SharedMemory};

use crate::error::Result;

/// The most times one run asks `agentsfleetd` for memory its window missed.
///
/// Each ask is a round trip the model waits on; past this a run answers from
/// what it already holds, so a model recalling in a loop costs the daemon a
/// bounded handful of searches rather than one per turn.
pub const RECALL_MISS_CAP: usize = 3;

/// Asks `agentsfleetd` for memory the hydrated window does not hold.
#[async_trait::async_trait]
pub trait Recall: Send + Sync + fmt::Debug {
    /// The fleet's entries holding `query` and, for a granted reader, the
    /// workspace's shared ones, at most `limit` of each.
    ///
    /// # Errors
    /// `agentsfleetd` did not answer; the run answers from its window.
    async fn recall(&self, query: &str, limit: usize) -> Result<MemoryRecallResponse<'static>>;
}

/// What `agentsfleetd` hydrated at lease start, borrowed from its reply.
#[derive(Debug, Clone, Copy, Default)]
pub struct Seed<'run> {
    /// The fleet's own window, newest first.
    pub window: &'run [MemoryDelta<'run>],
    /// The workspace's shared entries, newest first; empty without the read
    /// grant.
    pub shared: &'run [SharedMemory<'run>],
    /// Whether this fleet may store an entry the workspace reads.
    pub publish: bool,
    /// Where a recall the window cannot fill asks; `None` answers from the
    /// window alone.
    pub recall: Option<&'run dyn Recall>,
}

impl<'run> Seed<'run> {
    /// A seed holding only the fleet's own `window`.
    #[must_use]
    pub fn window(window: &'run [MemoryDelta<'run>]) -> Self {
        Self {
            window,
            ..Self::default()
        }
    }

    /// How many entries the fleet's own window holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.window.len()
    }

    /// Whether the fleet's own window is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.window.is_empty()
    }
}

/// One remembered entry, and — for one another fleet published — who wrote it.
///
/// Dereferences to the entry, so a caller reads `key` and `content` the way it
/// reads any delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recalled<'m> {
    /// The entry.
    pub delta: MemoryDelta<'m>,
    /// The writer's name for another fleet's shared entry; `None` for this
    /// fleet's own.
    pub writer: Option<Cow<'m, str>>,
}

impl<'m> Recalled<'m> {
    /// This fleet's own entry.
    #[must_use]
    pub const fn own(delta: MemoryDelta<'m>) -> Self {
        Self {
            delta,
            writer: None,
        }
    }

    /// Another fleet's shared entry, viewing `shared`'s text.
    #[must_use]
    pub fn shared(shared: &'m SharedMemory<'_>) -> Self {
        Self {
            delta: MemoryDelta {
                key: Cow::Borrowed(&shared.key),
                content: Cow::Borrowed(&shared.content),
                category: Cow::Borrowed(&shared.category),
                visibility: afd_wire::memory::Visibility::Workspace,
            },
            writer: Some(Cow::Borrowed(&shared.writer_fleet_name)),
        }
    }

    /// Another fleet's shared entry, owning `shared`'s text.
    #[must_use]
    pub fn shared_owned(shared: SharedMemory<'static>) -> Recalled<'static> {
        Recalled {
            delta: MemoryDelta {
                key: shared.key,
                content: shared.content,
                category: shared.category,
                visibility: afd_wire::memory::Visibility::Workspace,
            },
            writer: Some(shared.writer_fleet_name),
        }
    }

    /// Whether `other` names the same entry: one writer, one key.
    #[must_use]
    pub fn same_entry(&self, other: &Recalled<'_>) -> bool {
        self.delta.key == other.delta.key && self.writer == other.writer
    }
}

impl<'m> Deref for Recalled<'m> {
    type Target = MemoryDelta<'m>;

    fn deref(&self) -> &Self::Target {
        &self.delta
    }
}
