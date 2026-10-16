//! The HTTP seam the workspace fleets surface acts through.
//!
//! One trait over what the shared fleet store still answers — list, live set,
//! counters, ingress status and purge — because they are one store and a suite
//! that stubbed them separately would be stubbing an implementation detail.
//! Setup writes (Save, Apply, rename, status) belong to the native setup
//! authority, `afd_authoring::Creations`, never to this seam. Every method
//! takes ALREADY-PARSED values, so no implementation carries a validation arm.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use afd_api_wire::tail::FleetCounters;
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::{After, FleetPage, FleetStatus, Result as FleetResult};

/// Everything the workspace fleets routes act through.
///
/// A trait rather than the concrete store for the reason every seam in this
/// module is one: the router suites prove the refusal matrix in FRONT of the
/// verbs, and a matrix that needed a live Postgres AND a live Dragonfly to prove
/// would not be proven.
pub trait WorkspaceFleets: Send + Sync + std::fmt::Debug + 'static {
    /// One page of a workspace's fleets, newest first.
    ///
    /// `limit` is the page size the caller is served; the walk fetches one more
    /// than that to decide whether a next page exists.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon cannot
    /// read — including a status a newer build wrote.
    fn page(
        &self,
        workspace: &Uuid7,
        after: Option<&After>,
        limit: u32,
    ) -> impl Future<Output = FleetResult<FleetPage>> + Send;

    /// Whether this fleet will take new work, without reading what it is.
    ///
    /// `Ok(None)` for a fleet this workspace does not hold, which the caller
    /// renders as 404 — the statement is workspace-scoped, so a fleet somebody
    /// else owns and one that never existed are one answer.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn ingress_status(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = FleetResult<Option<FleetStatus>>> + Send;

    /// Every fleet this workspace holds, by identifier.
    ///
    /// The live wall's tick. Cached in the store rather than per connection —
    /// the set is a property of the workspace, so one enumeration serves every
    /// viewer of it. Whether THIS caller may see them is decided per request by
    /// the ownership layer and is never cached with the set.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. An empty workspace is an
    /// empty set.
    fn live_set(
        &self,
        workspace: &Uuid7,
    ) -> impl Future<Output = FleetResult<Arc<BTreeSet<String>>>> + Send;

    /// Where every fleet of `workspace` in `fleets` stands: the counter
    /// snapshot a `hello` carries so a subscriber is right before its first
    /// frame.
    ///
    /// Never cached, and asked only when a `hello` is about to go out: the
    /// set may be a tick old, but a counter that old is the staleness the
    /// frame exists to remove. Every identifier the workspace holds answers,
    /// zeros for a fleet that has never run; one it does not hold is absent.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn counters(
        &self,
        workspace: &Uuid7,
        fleets: &[String],
    ) -> impl Future<Output = FleetResult<BTreeMap<String, FleetCounters>>> + Send;

    /// Purges one killed fleet and everything keyed to it.
    ///
    /// # Errors
    /// Refuses an id naming no fleet this workspace holds, and one nobody
    /// killed first. Reports a datastore that would not answer.
    fn purge_tagged(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        expected: &str,
    ) -> impl Future<Output = FleetResult<()>> + Send;
}

/// The production store answers it directly.
impl WorkspaceFleets for afd_fleet_lifecycle::Fleets {
    fn page(
        &self,
        workspace: &Uuid7,
        after: Option<&After>,
        limit: u32,
    ) -> impl Future<Output = FleetResult<FleetPage>> + Send {
        Self::page(self, workspace, after, limit)
    }

    fn ingress_status(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = FleetResult<Option<FleetStatus>>> + Send {
        Self::ingress_status(self, workspace, fleet)
    }

    fn live_set(
        &self,
        workspace: &Uuid7,
    ) -> impl Future<Output = FleetResult<Arc<BTreeSet<String>>>> + Send {
        Self::live_set(self, workspace)
    }

    fn counters(
        &self,
        workspace: &Uuid7,
        fleets: &[String],
    ) -> impl Future<Output = FleetResult<BTreeMap<String, FleetCounters>>> + Send {
        Self::counters(self, workspace, fleets)
    }

    fn purge_tagged(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        expected: &str,
    ) -> impl Future<Output = FleetResult<()>> + Send {
        Self::purge_tagged(self, workspace, fleet, expected)
    }
}
