//! The HTTP seam the fleet memory routes act through.
//!
//! One trait over the page, the forget and the access grants, because they are
//! one store and a suite that stubbed them apart would be stubbing an
//! implementation detail.
//!
//! # Every method takes the workspace, and none is a filter
//!
//! The store proves the fleet is the workspace's itself, reading `core.fleets`
//! under the api role before any memory store is reached. Passing the
//! workspace here rather than resolving it in the handler is what makes the
//! check impossible to forget.
//!
//! # There is no store verb, and there never was one to port
//!
//! The tenant POST was retired with the runner-push cutover — a fleet remembers
//! what it LEARNED, never what a caller asserted — so the mutations here are the
//! operator's forget and the admin's grants.

use afd_core::id::Uuid7;
use afd_memory::page::{After, View};
use afd_memory::{Memories, Record, Result as MemoryResult};
use afd_wire::fleet::{MemoryAccess, MemoryAccessRequest};

/// Everything the fleet memory routes act through.
pub trait FleetMemories: Send + Sync + std::fmt::Debug + 'static {
    /// One page of a fleet's memory under `view`, newest first, holding the
    /// workspace's shared entries too when the fleet may read them.
    ///
    /// # Errors
    /// Refuses a fleet this workspace does not hold, reports a memory store
    /// that would not answer, and reports a row this daemon cannot read.
    fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> impl Future<Output = MemoryResult<Vec<Record>>> + Send;

    /// Removes one of the fleet's own entries, and refuses a key it is not
    /// holding.
    ///
    /// # Errors
    /// As [`Self::page`], plus the absent key — a refusal rather than a silent
    /// success, so an operator who mistyped learns the fleet still carries it.
    fn forget(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        key: &str,
    ) -> impl Future<Output = MemoryResult<()>> + Send;

    /// Sets the fleet's shared-memory grants, answering both as they stand.
    ///
    /// # Errors
    /// Refuses a fleet this workspace does not hold, and reports a database
    /// that would not answer.
    fn set_access(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        change: MemoryAccessRequest,
    ) -> impl Future<Output = MemoryResult<MemoryAccess>> + Send;
}

/// The production store answers every verb directly.
impl FleetMemories for Memories {
    fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> impl Future<Output = MemoryResult<Vec<Record>>> + Send {
        Self::page(self, workspace, fleet, view, after, limit)
    }

    fn forget(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        key: &str,
    ) -> impl Future<Output = MemoryResult<()>> + Send {
        Self::forget(self, workspace, fleet, key)
    }

    fn set_access(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        change: MemoryAccessRequest,
    ) -> impl Future<Output = MemoryResult<MemoryAccess>> + Send {
        Self::set_access(self, workspace, fleet, change)
    }
}
