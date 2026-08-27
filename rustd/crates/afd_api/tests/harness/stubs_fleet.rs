//! The fleet lifecycle's stub: the one seam that spans two datastores.
//!
//! Its own file rather than another entry in [`super::stubs_tenant`], because
//! it is another CRATE's seam and answers another crate's error. Grouping it
//! with the tenant plane's stubs would make one file the place where every stub
//! lives, which is how a reader stops being able to tell which plane a refusal
//! came from.

use afd_api::services::WorkspaceFleets;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

/// A fleet store with neither Postgres nor Redis behind it.
///
/// Every verb answers the refusal a datastore that would not answer produces,
/// for [`NoKeys`]' reason and one more of its own. The install's whole
/// behaviour is an INSERT, an `XGROUP CREATE` and a guarded flip against two
/// live datastores, and the guarantee it exists to make — the stream is there
/// before the 201 — is not something a stub can hold. Inventing a success would
/// be inventing that guarantee.
///
/// What a suite here proves is the guard, the scope rung, the ownership layer
/// and the body and query refusals in FRONT of the verbs, which is exactly
/// where §3's ownership dimension lives.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoFleets;

impl NoFleets {
    /// The refusal every verb below answers with.
    fn unavailable<T>() -> afd_fleet_lifecycle::Result<T> {
        Err(afd_fleet_lifecycle::Error::datastore_unavailable())
    }
}

impl WorkspaceFleets for NoFleets {
    fn page(
        &self,
        _workspace: &Uuid7,
        _after: Option<&afd_fleet_lifecycle::After>,
        _limit: u32,
    ) -> impl Future<Output = afd_fleet_lifecycle::Result<afd_fleet_lifecycle::FleetPage>> + Send
    {
        std::future::ready(Self::unavailable())
    }

    fn detail(
        &self,
        _workspace: &Uuid7,
        _fleet: &Uuid7,
    ) -> impl Future<Output = afd_fleet_lifecycle::Result<afd_fleet_lifecycle::FleetDetail>> + Send
    {
        std::future::ready(Self::unavailable())
    }

    fn install(
        &self,
        _workspace: &Uuid7,
        _request: &afd_fleet_lifecycle::Install<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet_lifecycle::Result<afd_fleet_lifecycle::Installed>> + Send
    {
        std::future::ready(Self::unavailable())
    }

    fn patch(
        &self,
        _workspace: &Uuid7,
        _fleet: &Uuid7,
        _request: &afd_fleet_lifecycle::Patch,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet_lifecycle::Result<afd_fleet_lifecycle::Patched>> + Send
    {
        std::future::ready(Self::unavailable())
    }

    fn purge(
        &self,
        _workspace: &Uuid7,
        _fleet: &Uuid7,
    ) -> impl Future<Output = afd_fleet_lifecycle::Result<()>> + Send {
        std::future::ready(Self::unavailable())
    }
}
