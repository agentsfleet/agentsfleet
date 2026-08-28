//! The seam the workspace fleets surface acts through.
//!
//! One trait over the whole lifecycle — list, install, read, edit, purge —
//! because they are one store and a suite that stubbed them separately would be
//! stubbing an implementation detail. Every method takes ALREADY-PARSED values:
//! a [`FleetName`] cannot hold a space, a [`Requested`] cannot mean `paused`,
//! and a [`ConfigSource`] cannot be both a document and a configuration. So
//! there is no validation arm in any implementation, and none a stub could get
//! differently right from the real one.

use afd_core::clock::UnixMillis;
use std::collections::BTreeSet;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::{
    After, FleetDetail, FleetPage, FleetStatus, Install, Patch, Patched, Result as FleetResult,
};

/// Everything the workspace fleets routes act through.
///
/// A trait rather than the concrete store for the reason every seam in this
/// module is one: the router suites prove the refusal matrix in FRONT of the
/// verbs, and a matrix that needed a live Postgres AND a live Redis to prove
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

    /// One fleet of the workspace, whole.
    ///
    /// # Errors
    /// Refuses an id naming no fleet THIS workspace holds — the statement is
    /// workspace-scoped, so a fleet somebody else owns and one that never
    /// existed are indistinguishable, and neither is disclosed. Reports a
    /// datastore that would not answer.
    fn detail(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = FleetResult<FleetDetail>> + Send;

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

    /// Installs one fleet, its event stream and consumer group included.
    ///
    /// # Errors
    /// Refuses a library id naming nothing installable here, either authored
    /// document being unusable, the two naming different fleets, and a CHOSEN
    /// name the workspace already holds. Reports an install whose stream could
    /// not be created — with the row removed — and a datastore that would not
    /// answer.
    fn install(
        &self,
        workspace: &Uuid7,
        request: &Install<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = FleetResult<afd_fleet_lifecycle::Installed>> + Send;

    /// Applies one PATCH inside a single row lock.
    ///
    /// # Errors
    /// Refuses an id naming no fleet this workspace holds and one already
    /// killed, a transition the machine does not allow from where the row
    /// stands, and an `If-Match` naming a version the source has moved past.
    /// Reports a lock this request could not take, and a datastore that would
    /// not answer.
    fn patch(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        request: &Patch,
        now: UnixMillis,
    ) -> impl Future<Output = FleetResult<Patched>> + Send;

    /// Purges one killed fleet and everything keyed to it.
    ///
    /// # Errors
    /// Refuses an id naming no fleet this workspace holds, and one nobody
    /// killed first. Reports a datastore that would not answer.
    fn purge(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
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

    fn detail(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = FleetResult<FleetDetail>> + Send {
        Self::detail(self, workspace, fleet)
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

    fn install(
        &self,
        workspace: &Uuid7,
        request: &Install<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = FleetResult<afd_fleet_lifecycle::Installed>> + Send {
        Self::install(self, workspace, request, now)
    }

    fn patch(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        request: &Patch,
        now: UnixMillis,
    ) -> impl Future<Output = FleetResult<Patched>> + Send {
        Self::patch(self, workspace, fleet, request, now)
    }

    fn purge(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = FleetResult<()>> + Send {
        Self::purge(self, workspace, fleet)
    }
}
