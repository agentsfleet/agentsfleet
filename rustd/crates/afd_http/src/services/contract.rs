//! The seam every handler is written against.
//!
//! Split from `mod.rs` at the file cap, which now carries only the module index
//! and the re-exports — one file naming what this plane offers, one declaring
//! what it demands.
//!
//! The tenant-scoped half is a supertrait of its own next door. Nothing about
//! the seam changes for a caller: `Services` still resolves every accessor and
//! every associated type, because a supertrait's members are reachable through
//! the subtrait. What the split buys is that a tenant-surface accessor lands in
//! the file about the tenant surface.

use afd_admin::{Models as AdminModels, PlatformKeys};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::bundle::Bundles;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_library::{Libraries, LibraryImports};
use afd_observability::Analytics;
use afd_runner::Runners;
use afd_sse::Live;

use crate::auth::Authenticator;

use super::TenantSurface;
use super::{
    DeviceFlow, FleetGrants, FleetMemories, FleetSchedules, FleetSteering, Leasing, WebhookIngress,
    WorkspaceApprovals, WorkspaceConnectors, WorkspaceEvents, WorkspaceFleets, WorkspaceOwnership,
    WorkspacePreferences, WorkspaceSecrets,
};

/// The services one request is served through.
///
/// Implemented by the binary's composition root. A suite implements it too —
/// against an in-memory directory and a pool that answers nothing — which is
/// what puts the whole refusal matrix in a test with no datastore in it.
pub trait Services: TenantSurface + Send + Sync + std::fmt::Debug + 'static {
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

    /// What the workspace fleets surface acts through.
    ///
    /// An associated type for the reason [`Services::Leases`] is one: the
    /// concrete store holds a Redis connection opened by CONNECTING — the
    /// install's whole guarantee is that a stream exists before the 201 — so a
    /// suite proving the refusal matrix in front of these verbs cannot build
    /// one and must not need to.
    type Fleets: WorkspaceFleets;

    /// The workspace's fleets: list, install, read, edit, purge.
    fn fleets(&self) -> &Self::Fleets;

    /// What the workspace secret surface acts through.
    ///
    /// A concrete type where [`Services::Fleets`] is an associated one, and the
    /// difference is what each of them is over. The fleets store holds a Redis
    /// connection opened by CONNECTING, so a suite cannot build one; the vault
    /// holds a Postgres pool, an entropy source and a key, and every one of
    /// those has a seam a suite drives it through — `afd_db::Db::unreachable`,
    /// `Entropy::new_mocked` and `Kek::from_bytes`. The seam is inside the
    /// type, so it does not also need to be a parameter on this trait.
    type Secrets: WorkspaceSecrets;

    /// The workspace's secrets: store, list, replace, delete.
    fn secrets(&self) -> &Self::Secrets;

    /// What the approval inbox acts through.
    ///
    /// A concrete type for the reason [`Services::Secrets`] is one: a Postgres
    /// pool and nothing else, with `afd_db::Db::unreachable` as the seam.
    type Approvals: WorkspaceApprovals;

    /// The operator's queue of approval gates.
    fn approvals(&self) -> &Self::Approvals;

    /// What the integration-grant routes act through.
    ///
    /// A concrete type for the reason [`Services::Approvals`] is one: a
    /// Postgres pool and nothing else, with `afd_db::Db::unreachable` as the
    /// seam. Separate from the inbox beside it because the two are separate
    /// stores over separate tables — the queue also holds a Redis connection,
    /// for a continuation a grant decision never lands.
    type Grants: FleetGrants;

    /// A fleet's standing permissions to reach a third party.
    fn grants(&self) -> &Self::Grants;
    /// What the fleet memory routes act through.
    ///
    /// A concrete type for the reason [`Services::Approvals`] is one: a
    /// Postgres pool and an entropy source, both with the seam a suite drives
    /// them through. The SAME store the runner plane's capture writes with —
    /// production holds one `Memories`, because an operator reading what a
    /// fleet learned and a runner writing it are two verbs over one table.
    type Memories: FleetMemories;

    /// A fleet's durable memory: the page, and the forget.
    fn memories(&self) -> &Self::Memories;

    /// What the schedules surface and the fire ingress act through.
    ///
    /// An associated type for the reason [`Services::Ingress`] is one: the
    /// concrete plane holds a Redis connection and an outbound HTTP client, so
    /// a suite proving the refusal matrix in front of these routes cannot build
    /// one and must not need to.
    type Schedules: FleetSchedules;

    /// What a schedule is read, written and fired through.
    fn schedules(&self) -> &Self::Schedules;

    /// The scheduler's signing keys, when this deployment configured them.
    ///
    /// `None` is fail-closed: a daemon that cannot verify a fire refuses every
    /// one, because acting on an unverified callback would let anyone who found
    /// the URL wake every fleet behind it.
    fn schedule_signing_keys(&self) -> Option<&afd_cron::SigningKeys>;

    /// Where a fire is expected to arrive, as the token's `sub` claim spells it.
    ///
    /// Half of what makes a fire token this deployment's: a token minted for
    /// another daemon's destination fails the subject check rather than waking
    /// a fleet here.
    fn schedule_destination(&self) -> &str;

    /// What the signed-ingress routes act through.
    ///
    /// An associated type for the reason [`Services::Fleets`] is one: the
    /// concrete store holds a Redis connection opened by CONNECTING — the
    /// delivery's at-most-once claim and its append are one Lua script on it —
    /// so a suite proving the refusal matrix in front of these routes cannot
    /// build one and must not need to.
    type Ingress: WebhookIngress;

    /// What a signed delivery is resolved, verified and appended through.
    fn ingress(&self) -> &Self::Ingress;

    /// The workspace whose vault holds this deployment's own platform secrets.
    ///
    /// `None` for a deployment that configured none, which is a supported
    /// state: every surface that needs one fails closed rather than guessing,
    /// and the rest of the daemon runs without it.
    ///
    /// The App ingress is the reason this crosses the seam. Its signing secret
    /// belongs to the DEPLOYMENT rather than to any fleet or workspace — one
    /// App, one secret, configured once — so it is the one ingress secret that
    /// cannot be reached through a [`afd_ingress::Binding`], because it has to
    /// be verified before there is a binding to reach it through.
    fn platform_admin_workspace(&self) -> Option<&Uuid7>;

    /// What a signup event's signature is checked against.
    ///
    /// `None` refuses every delivery — see [`IdentityWebhookSecret`] on why
    /// fail-closed is the only safe answer for a route that creates accounts.
    fn identity_webhook_secret(&self) -> Option<&afd_crypto::secret::SecretBytes>;

    /// What the connector routes act through.
    ///
    /// An associated type for the reason [`Services::Ingress`] is one: the
    /// concrete flow holds a Redis connection opened by CONNECTING — a
    /// round-trip's single-use slot lives there — so a suite proving the
    /// refusal matrix in front of these routes cannot build one and must not
    /// need to.
    type Connectors: WorkspaceConnectors;

    /// What a connect is started, finished, read and forgotten through.
    fn connectors(&self) -> &Self::Connectors;

    /// Where a PERSON goes, as distinct from where this daemon answers.
    ///
    /// Beside [`Services::deployment`] and never the same string. Every
    /// connector redirect is built from this one: the `redirect_uri` a provider
    /// mints its code against, the relay the browser is sent back through, and
    /// the page a completed connect lands on. Reading it from a request's
    /// `Host` would let a provider's registered callback and this daemon's idea
    /// of it disagree, which fails as `redirect_uri_mismatch` at the vendor and
    /// reads like a rotated credential.
    fn dashboard(&self) -> &str;

    /// What the steer verb acts through.
    type Steering: FleetSteering;

    /// A fleet's message ingress.
    fn steering(&self) -> &Self::Steering;

    /// Where this instance's product events go.
    ///
    /// A concrete type for the reason [`Services::bundles`] is one: it carries
    /// its own absence, so a deployment reporting nothing is a value rather
    /// than a `None` each handler would have to branch on.
    fn analytics(&self) -> &Analytics;

    /// What both live-stream routes act through.
    ///
    /// A concrete type where [`Services::Leases`] is an associated one, and the
    /// difference is what each is over: a lease plane holds a Redis connection
    /// opened by CONNECTING, while this holds a pub/sub hub and a semaphore,
    /// and `afd_sse::Live::detached` already lets a suite build one with no
    /// server behind it. The seam is inside the type, so it does not also need
    /// to be a parameter on this trait — the same shape [`Services::bundles`]
    /// takes, and for the same reason.
    fn live(&self) -> &Live;

    /// What the event-history routes act through.
    ///
    /// A concrete type for the reason [`Services::Approvals`] is one: a
    /// Postgres pool and nothing else, with `afd_db::Db::unreachable` as the
    /// seam.
    type Events: WorkspaceEvents;

    /// The narrative log a workspace and its fleets wrote.
    fn events(&self) -> &Self::Events;

    /// What the preference and onboarding surfaces act through.
    ///
    /// A concrete type for the reason [`Services::Secrets`] is one: it holds a
    /// Postgres pool and an entropy source, and both carry the seam a suite
    /// drives them through.
    type Preferences: WorkspacePreferences;

    /// One person's dashboard preferences, and the checklist over them.
    fn preferences(&self) -> &Self::Preferences;

    /// Read-only cross-table projections for fleet operators.
    ///
    /// A concrete type for the reason [`Services::bundles`] is one: it holds a
    /// Postgres pool and nothing else, so `afd_db::Db::unreachable` is the
    /// whole of the seam a suite needs.
    fn runner_lease_history(&self) -> &RunnerLeaseHistory;

    /// Priced-model catalogue administration.
    ///
    /// Distinct from [`Services::catalogue`], which is the tenant's READ of the
    /// same rows: this is the admin plane's write, gated behind platform
    /// scopes, and keeping them apart is what stops a tenant route from
    /// reaching a mutation by holding the wrong accessor.
    fn models(&self) -> &AdminModels;

    /// Reveal-free platform-default administration.
    fn platform_keys(&self) -> &PlatformKeys;

    /// Platform Fleet-library catalogue administration.
    fn libraries(&self) -> &Libraries;

    /// Validated Fleet-library source and snapshot onboarding.
    fn library_imports(&self) -> &LibraryImports;

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
