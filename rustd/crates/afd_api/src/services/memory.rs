//! The seam the fleet memory routes act through.
//!
//! One trait over the page and the forget, because they are one store and a
//! suite that stubbed them apart would be stubbing an implementation detail.
//!
//! # Both methods take the workspace, and neither is a filter
//!
//! `memory.memory_entries` carries no workspace column, so the scoping is a
//! read of `core.fleets` the store performs itself — under the api role, before
//! it takes the `memory_runtime` role that cannot see that table. Passing the
//! workspace here rather than resolving it in the handler is what makes the
//! check impossible to forget: there is no method on this trait that will
//! answer for a fleet without being told whose it must be.
//!
//! # There is no store verb, and there never was one to port
//!
//! The tenant POST was retired with the runner-push cutover — a fleet remembers
//! what it LEARNED, never what a caller asserted — so the only mutation here is
//! the operator's forget.

use afd_core::id::Uuid7;
use afd_fleet::Result as FleetResult;
use afd_fleet::memory::Memories;
use afd_fleet::memory::page::{After, Entry, View};

/// Everything the fleet memory routes act through.
pub trait FleetMemories: Send + Sync + std::fmt::Debug + 'static {
    /// One page of a fleet's memory under `view`, newest first.
    ///
    /// # Errors
    /// Refuses a fleet this workspace does not hold, reports a memory backend
    /// that would not answer, and reports a row this daemon cannot read. The
    /// view, the boundary and the limit are resolved by the handler, so nothing
    /// here is the caller's fault.
    fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> impl Future<Output = FleetResult<Vec<Entry>>> + Send;

    /// Removes one entry, and refuses a key the fleet is not holding.
    ///
    /// # Errors
    /// As [`Self::page`], plus the absent key — which is a refusal rather than
    /// a silent success, so an operator who mistyped learns the fleet is still
    /// carrying the lesson.
    fn forget(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        key: &str,
    ) -> impl Future<Output = FleetResult<()>> + Send;
}

/// The production store answers both directly.
impl FleetMemories for Memories {
    fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> impl Future<Output = FleetResult<Vec<Entry>>> + Send {
        Self::page(self, workspace, fleet, view, after, limit)
    }

    fn forget(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        key: &str,
    ) -> impl Future<Output = FleetResult<()>> + Send {
        Self::forget(self, workspace, fleet, key)
    }
}
