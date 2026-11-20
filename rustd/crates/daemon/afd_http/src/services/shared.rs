//! What every plane's handlers share: the guard's authenticator, the
//! ownership resolver, and the request clock.
//!
//! Each plane trait (`afd_api_tenant::TenantPlane`, `afd_api_runner::RunnerPlane`,
//! `afd_api_operator::OperatorPlane`, `afd_api_ingress::IngressPlane`) is a
//! subtrait of this one, so the guard and ownership layers this crate mounts
//! are written once against it, and a plane names nothing another plane uses.

use jiff::Timestamp;

use crate::auth::Authenticator;

use super::WorkspaceOwnership;

/// The services the shared guard, the ownership layer and every handler reach.
///
/// Implemented by the binary's composition root. A suite implements it too —
/// against an in-memory directory and a pool that answers nothing — which is
/// what puts the whole refusal matrix in a test with no datastore in it.
pub trait Shared: Send + Sync + std::fmt::Debug + 'static {
    /// What proves a credential on either plane.
    type Auth: Authenticator;

    /// The authenticator every guarded route is proven against.
    fn authenticator(&self) -> &Self::Auth;

    /// What decides whose workspace a request is acting in.
    ///
    /// A concrete type where a run plane is an associated one, and the
    /// difference is what each is over: a run plane holds a bus connection
    /// beside its stores, while this holds a Postgres pool, which
    /// `afd_db::Db::unreachable` already lets a suite build without a server.
    type Workspaces: WorkspaceOwnership;

    /// The workspace-ownership resolver the shared layer asks.
    fn workspaces(&self) -> &Self::Workspaces;

    /// The instant this request's writes are stamped with.
    ///
    /// Read ONCE per verb and threaded through it, so every row one request
    /// writes carries the same instant. Reading the clock again for each write
    /// would leave, say, a heartbeat's liveness stamp a millisecond or two
    /// after its own transition event.
    fn now(&self) -> Timestamp;
}
