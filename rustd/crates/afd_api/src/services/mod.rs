//! What a HANDLER reaches for, as distinct from what `/readyz` consults.
//!
//! Two traits over one state value, split where the seam actually is.
//! [`crate::router::Dependencies`] answers "can this instance take traffic",
//! which is a question about connections; this one answers "what does this verb
//! act through", which is a question about services. A probe that grew a
//! `runners()` method would be asking a readiness check to know about the
//! runner plane.
//!
//! One file per plane, because the seams are one per plane too: the runner
//! plane's verbs in [`leasing`], the device-flow login in [`device_flow`], the
//! tenant's own credentials and ownership in [`tenant`], and the tenant's
//! money in [`billing`]. Each trait is re-exported here, so a handler still
//! names `crate::services::TenantKeys` and never a file.
//!
//! # Why the state is a trait and not a struct
//!
//! The authenticator's concrete type carries three parameters — a directory, a
//! capability source, and a token verifier — and every one of them is chosen by
//! the binary, not by this crate. A concrete state struct would put all three
//! on `build`, on every handler signature, and on every test fixture. One
//! associated type collapses them, and the request path still costs no virtual
//! call because the trait is taken as a generic parameter (`M-DI-HIERARCHY`).
//!
//! # Why the clock is here
//!
//! `afd_core::clock` asks callers to take an instant as a PARAMETER wherever
//! the decision can be handed one, and reserves injection for a long-lived
//! owner that reads repeatedly. The router is that owner: it lives for the
//! process, and every verb under it needs the instant its writes are stamped
//! with. Reading the wall clock inside each handler instead would put a
//! non-deterministic call in the one place a test most needs to pin.

mod billing;
mod catalogue;
mod device_flow;
mod leasing;
mod tenant;

pub use self::billing::TenantBilling;
pub use self::catalogue::ModelCatalogue;
pub use self::device_flow::DeviceFlow;
pub use self::leasing::Leasing;
pub use self::tenant::{TenantKeys, TenantWorkspaces, TerminalCredentials, WorkspaceOwnership};

use afd_core::clock::UnixMillis;
use afd_fleet::Runners;
use afd_fleet::bundle::Bundles;

use crate::auth::Authenticator;

/// The services one request is served through.
///
/// Implemented by the binary's composition root. A suite implements it too —
/// against an in-memory directory and a pool that answers nothing — which is
/// what puts the whole refusal matrix in a test with no datastore in it.
pub trait Services: Send + Sync + std::fmt::Debug + 'static {
    /// What proves a credential on either plane.
    type Auth: Authenticator;

    /// The authenticator every guarded route is proven against.
    fn authenticator(&self) -> &Self::Auth;

    /// The runner control plane's store.
    fn runners(&self) -> &Runners;

    /// What the lease verb acts through.
    ///
    /// An associated type for the reason [`Services::Auth`] is one: the
    /// concrete plane holds a Redis connection that is opened by CONNECTING,
    /// so a suite proving the router's refusal matrix cannot construct one and
    /// must not need to. The binary supplies `afd_fleet::lease::Plane`; a test
    /// supplies whatever answers.
    type Leases: Leasing;

    /// The lease verb's plane: claims, gates, money, credentials.
    ///
    /// Separate from [`Services::runners`] because they are different
    /// questions over different tables — the runner store answers "what is
    /// this host", and this answers "what may it run next". A single accessor
    /// returning both would put the money path behind every heartbeat.
    fn leases(&self) -> &Self::Leases;

    /// The Fleet Bundle snapshot store.
    ///
    /// A concrete type where [`Services::Leases`] is an associated one, and the
    /// difference is what each of them is over. A lease plane holds a Redis
    /// connection opened by CONNECTING, so a suite cannot build one; a bundle
    /// store holds an `Arc<dyn ObjectStore>`, and `object_store` already ships
    /// the in-memory backend a suite drives it with. The seam is inside the
    /// type, so it does not also need to be a parameter on this trait.
    ///
    /// Not an `Option`. A deployment with no snapshot storage answers
    /// `Bundles::unconfigured`, which refuses with a registry code and a
    /// sentence like every other failure on this plane — see
    /// [`afd_fleet::bundle::Bundles`] for why the absence is a value rather
    /// than a `None` each handler would have to render for itself.
    fn bundles(&self) -> &Bundles;

    /// What the device-flow login surface acts through.
    ///
    /// An associated type for the reason [`Services::Leases`] is one: the
    /// concrete surface holds a Redis connection opened by CONNECTING, and a
    /// suite proving the router's refusal matrix must not need one.
    type Sessions: DeviceFlow;

    /// The device-flow login surface.
    fn sessions(&self) -> &Self::Sessions;

    /// What decides whose workspace a request is acting in.
    ///
    /// A concrete type where [`Services::Leases`] is an associated one, and the
    /// difference is what each is over: a lease plane holds a Redis connection
    /// opened by CONNECTING, while this holds a Postgres pool, which
    /// `afd_db::Db::unreachable` already lets a suite build without a server.
    /// The seam is inside the type, so it does not also need to be a parameter
    /// on this trait.
    type Workspaces: WorkspaceOwnership;

    /// The workspace-ownership resolver the shared layer asks.
    fn workspaces(&self) -> &Self::Workspaces;

    /// The tenant's workspace directory — its list, and the create.
    ///
    /// A second seam over what production holds as ONE value, and the split
    /// is the suites': the ownership stub owns exactly one workspace so both
    /// halves of the refusal matrix are reachable, while this one refuses
    /// like every other store. See [`TenantWorkspaces`] for the longer form.
    type WorkspaceDirectory: TenantWorkspaces;

    /// The workspace directory the tenant-plane verbs act through.
    fn workspace_directory(&self) -> &Self::WorkspaceDirectory;

    /// The tenant's own api-keys.
    ///
    /// A concrete type for the reason [`Services::Workspaces`] is one: it holds
    /// a Postgres pool and an entropy source, and both can be built without a
    /// server — `afd_db::Db::unreachable` and `Entropy::new_mocked` are the
    /// seams a suite drives it through.
    type ApiKeys: TenantKeys;

    /// The tenant api-key store.
    fn api_keys(&self) -> &Self::ApiKeys;

    /// A person's own command-line credentials.
    ///
    /// A concrete type for the reason [`Services::ApiKeys`] is one: it holds a
    /// Postgres pool and an entropy source, and both can be built without a
    /// server.
    type CliCredentials: TerminalCredentials;

    /// The command-line credential store.
    fn cli_credentials(&self) -> &Self::CliCredentials;

    /// The tenant's billing reads.
    ///
    /// A concrete type for the reason [`Services::Workspaces`] is one: it
    /// holds a Postgres pool and nothing else, and `afd_db::Db::unreachable`
    /// is the seam a suite drives it through.
    type Billing: TenantBilling;

    /// The billing read surface.
    fn billing(&self) -> &Self::Billing;

    /// The priced model catalogue's read surface.
    ///
    /// A concrete type for the reason [`Services::Billing`] is one: a
    /// Postgres pool and nothing else.
    type Catalogue: ModelCatalogue;

    /// The catalogue the `/v1/models` read acts through.
    fn catalogue(&self) -> &Self::Catalogue;

    /// This deployment's own base URL, as a minted credential records it.
    ///
    /// A method on the plane rather than a value a handler reads from the
    /// request, and that is the whole point: a credential and the deployment
    /// that minted it are ONE fact, so a client-asserted `Host` header would
    /// let the two disagree. `serve_broker.zig` and `runtime_loader.zig` read
    /// the same knob for the same reason.
    fn deployment(&self) -> &str;

    /// The instant this request's writes are stamped with.
    ///
    /// Read ONCE per verb and threaded through it, so every row one request
    /// writes carries the same instant — the property `heartbeat.zig` loses by
    /// calling `clock.nowMillis()` separately in each of its four writes, which
    /// leaves a beat's liveness stamp a millisecond or two after its own
    /// transition event.
    fn now(&self) -> UnixMillis;
}
