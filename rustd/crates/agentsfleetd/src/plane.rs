//! The composition root: every seam the router is generic over, chosen once.
//!
//! `afd_api` names no directory, no capability provider and no store — it takes
//! them as type parameters behind two traits, so the HTTP shell can be driven
//! by an in-memory directory in a test and by Postgres in production without
//! either knowing about the other. This is the file where production says which
//! is which, and it is the ONLY file that does.
//!
//! # Why the probe half stays its own type
//!
//! [`LiveDependencies`] answers `/readyz` and nothing else. It could have been
//! folded in here — it is two handles — and it is not, because the two halves
//! answer different questions: a probe reports whether an instance should take
//! traffic, and the services below are what a verb acts through. A readiness
//! check that grew a `runners()` method would be a probe that knows about the
//! runner plane.

use std::sync::Arc;

// Aliased: `afd_tenant::models::Models` below is the tenant's READ of the
// priced catalogue, and this is the admin plane's WRITE of the same rows. Two
// things called `Models` in one file is how a reader ends up believing the
// tenant surface can mutate the catalogue.
use afd_admin::{Models as AdminModels, PlatformKeys};
use afd_api::Planes;
use afd_api::router::{Dependencies, ReadyInputs};
use afd_approval::{Inbox, IntegrationGrants};
use afd_billing::Accounts;
use afd_credential::provider::Providers;
use afd_credential::secrets::Registry;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_events::History;
use afd_fleet::bundle::Bundles;
use afd_fleet::lease::{Leases, Plane};
use afd_fleet::memory::Memories;
use afd_fleet_lifecycle::Fleets;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_gate::gate::Gates;
use afd_library::{Libraries, LibraryImports};
use afd_runner::Runners;
use afd_tenant::preference::Preferences;
// Aliased: `crate::identity::Sessions` is the token VERIFIER, and this is the
// device-flow login surface. Two things called `Sessions` in one file is how a
// reader ends up believing the login surface verifies bearer tokens.
use afd_billing::tenant::Billing;
use afd_credential::vault::Vault;
use afd_observability::Analytics;
use afd_redis::Redis;
use afd_sse::Live;
use afd_state::Credentials;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
// Aliased: `afd_credential::vault::Vault` above is the RUNNER plane's reader — it
// opens a credential a fleet declared and never lists — and this is the
// workspace-admin surface that seals, lists without a key, and deletes under
// the model-registry lock. Two things called `Vault` in one file is how a
// reader ends up believing one of them can do the other's job.
use afd_api::SchedulePlane;
use afd_core::id::Uuid7;
use afd_cron::{Fire, QStash, ScheduleService, Schedules, SigningKeys};
use afd_ingress::Ingress;
use afd_vault::Vault as SecretVault;

use crate::bundles::Stores;

use crate::identity::{Capabilities, Sessions};
use crate::probes::LiveDependencies;

/// The authenticator this daemon serves both planes through.
///
/// Spelled once, because the three parameters appear in the state type, in the
/// associated type, and at construction — and a reader should have to reconcile
/// them in one place rather than three.
pub type Authenticator = Planes<Credentials, Capabilities, Sessions>;

/// Everything one request is served through, in production.
#[derive(Debug)]
pub struct ServingPlane {
    probes: LiveDependencies,
    authenticator: Authenticator,
    runners: Runners,
    leases: Plane,
    bundles: Bundles,
    logins: Logins,
    workspaces: Workspaces,
    fleets: Fleets,
    api_keys: ApiKeys,
    cli_credentials: CliCredentials,
    billing: Billing,
    models: Models,
    admin_models: AdminModels,
    platform_keys: PlatformKeys,
    libraries: Libraries,
    library_imports: LibraryImports,
    runner_lease_history: RunnerLeaseHistory,
    secrets: SecretVault,
    preferences: Preferences,
    approvals: Inbox,
    grants: IntegrationGrants,
    events: History,
    steering: afd_events::Steer,
    ingress: Ingress,
    schedules: SchedulePlane,
    connectors: afd_connector::Connectors,
    schedule_keys: Option<SigningKeys>,
    schedule_destination: String,
    /// What a signup event from the identity provider is verified against.
    identity_webhook_secret: Option<SecretBytes>,
    /// Opening a personal account from a verified signup event.
    signups: afd_tenant::signup::Signups,
    platform_admin_workspace: Option<Uuid7>,
    live: Live,
    analytics: Analytics,
    api_url: Box<str>,
    app_url: String,
}

impl ServingPlane {
    /// Assembles the plane over already-connected pools.
    ///
    /// Takes them CONNECTED rather than taking configuration, for the reason
    /// `LiveDependencies` does: boot has already proven they answer, and a
    /// second way to reach a datastore is a second thing to drift.
    ///
    /// The credential directory and the runner store share the api-role pool
    /// deliberately. Both are on the request path — the directory on every
    /// authenticated call, the store on every runner verb — so a separate pool
    /// for either would let one starve while the other sat idle.
    ///
    /// The snapshot store arrives BUILT, and possibly empty — `Bundles` holds
    /// its own absence, so a deployment with no R2 knobs hands over
    /// `Bundles::unconfigured` rather than an `Option` this file would have to
    /// unwrap into a refusal each handler re-invented.
    ///
    /// The broker arrives BUILT, for the reason the snapshot store does: which
    /// platform credentials this deployment holds is read from the vault at
    /// boot, and that read is asynchronous where this constructor is not.
    ///
    /// The KEK arrives already shared. `preflight` resolved and validated it
    /// before this point and refuses boot without one, so every store below
    /// that opens a sealed row takes the SAME key — which is Milestone
    /// Invariant 3 as an ownership fact rather than as a rule about who reads
    /// which variable.
    #[must_use]
    pub fn new(parts: PlaneParts) -> Self {
        let PlaneParts {
            database,
            identity_webhook_secret,
            queue,
            kek,
            capabilities,
            sessions,
            stores,
            broker,
            platform_admin_workspace,
            live,
            analytics,
            login,
            schedule,
        } = parts;
        // One object-store owner, split into the half that READS a snapshot and
        // the half that WRITES one. A deployment with no upload handle still
        // serves the catalogue; `LibraryImports::without_store` carries that
        // absence as a value, the way `Bundles::unconfigured` does.
        // The exchange dials the vendor and the Jira site listing dials it
        // again, so both take the same handle: one pool for everything this
        // family sends outbound — see `crate::credentials`.
        let vendor_client = crate::credentials::vendor_exchange_client();
        let (bundles, uploads) = stores.split();
        let library_imports = match uploads {
            Some(store) => LibraryImports::new(database.clone(), store),
            None => LibraryImports::without_store(database.clone()),
        };
        Self {
            bundles,
            library_imports,
            platform_admin_workspace,
            runner_lease_history: RunnerLeaseHistory::new(database.clone()),
            admin_models: AdminModels::new(database.clone(), Entropy::new()),
            platform_keys: PlatformKeys::new(database.clone()),
            libraries: Libraries::new(database.clone()),
            workspaces: Workspaces::new(database.clone(), Entropy::new()),
            fleets: Fleets::new(database.clone(), queue.clone(), Entropy::new()),
            api_keys: ApiKeys::new(database.clone(), Entropy::new()),
            cli_credentials: CliCredentials::new(database.clone(), Entropy::new()),
            billing: Billing::new(database.clone()),
            models: Models::new(database.clone()),
            secrets: SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), queue.clone()),
            grants: IntegrationGrants::new(database.clone()),
            events: History::new(database.clone()),
            steering: afd_events::Steer::new(queue.clone()),
            // The SAME key every other sealing store takes, so the signing
            // secret a webhook is checked against opens under the key the
            // workspace surface sealed it with. A second `SecretVault` value
            // rather than a share of the `secrets` field above because the two
            // are different surfaces over one table — that one seals and never
            // opens, this one opens exactly one name and never lists.
            ingress: Ingress::new(
                database.clone(),
                SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
                queue.clone(),
            ),
            // The destination is derived from the API url this deployment
            // already knows, so a schedule registered upstream calls back to
            // the daemon that registered it. A url carrying a query or a
            // fragment is refused at construction rather than silently
            // truncating the callback — see `qstash::destination_url`.
            schedule_destination: schedule.destination.clone(),
            identity_webhook_secret,
            signups: afd_tenant::signup::Signups::new(database.clone(), Entropy::new()),
            schedule_keys: schedule.keys,
            schedules: SchedulePlane::new(
                ScheduleService::new(
                    Schedules::new(database.clone(), Entropy::new()),
                    QStash::new(
                        schedule.client,
                        schedule.token,
                        schedule.destination,
                        schedule.api_base,
                    ),
                ),
                Fire::new(queue.clone()),
                Entropy::new(),
            ),
            // The SAME key every other sealing store takes, twice over and
            // deliberately: the platform half opens this deployment's own
            // `<provider>-app` bags in the admin workspace, and the grant half
            // seals a tenant's handle in theirs. Two `Vault` values over one
            // table, for the reason the ingress beside them is two — a reader
            // of the deployment's credentials and a writer of a workspace's are
            // different surfaces, and one value serving both would let a
            // connector route reach the wrong workspace's secrets by holding
            // the wrong handle.
            connectors: afd_connector::Connectors::new(
                afd_connector::PlatformApp::new(SecretVault::new(
                    database.clone(),
                    Arc::clone(&kek),
                    Entropy::new(),
                )),
                afd_connector::Grants::new(
                    SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
                    database.clone(),
                    Entropy::new(),
                ),
                afd_connector::Exchange::new(vendor_client.clone()),
                vendor_client,
                queue.clone(),
                Entropy::new(),
            ),
            live,
            analytics,
            api_url: login.api_url,
            logins: Logins::new(
                afd_redis::SessionStore::new(queue.clone()),
                login.code_pepper,
                Entropy::new(),
                &login.app_url,
            ),
            // After `logins` above, which BORROWS it: a struct literal
            // evaluates its fields in order, so moving it first would leave
            // nothing for the device-flow surface to read.
            app_url: login.app_url,
            probes: LiveDependencies::new(database.clone(), queue.clone()),
            authenticator: Planes::new(Credentials::new(database.clone()), capabilities, sessions),
            runners: Runners::new(database.clone(), Entropy::new()),
            leases: Plane {
                leases: Leases::new(database.clone(), queue.clone(), Entropy::new()),
                gates: Gates::new(database.clone(), queue, Entropy::new()),
                accounts: Accounts::new(database.clone(), Entropy::new()),
                memories: Memories::new(database.clone(), Entropy::new()),
                providers: Providers::new(database.clone(), Arc::clone(&kek)),
                vault: Vault::new(database, kek),
                broker,
                connectors: Registry::default(),
            },
        }
    }
}

/// Everything [`ServingPlane::new`] is assembled from.
///
/// A parameter object rather than eight positional arguments, and not only to
/// satisfy a lint. Each field is CONNECTED or BUILT before it gets here, which
/// is the property the constructor's own note is about — boot has already
/// proven the pools answer, resolved the snapshot store's absence into a value,
/// and read this deployment's platform credentials out of the vault. Naming
/// them at the call site is what makes that readable in one place.
#[derive(Debug)]
pub struct PlaneParts {
    /// The API role's Postgres pool, open and proven.
    pub database: Db,
    /// The API role's Redis, open and proven.
    pub queue: Redis,
    /// The master key every stored credential is sealed under.
    ///
    /// Already shared: `preflight` resolved and validated it and refuses boot
    /// without one, so every store below that opens a sealed row takes the SAME
    /// key — Milestone Invariant 3 as an ownership fact rather than as a rule
    /// about who reads which variable.
    pub kek: Arc<Kek>,
    /// Where a subject's capability claim is read from.
    pub capabilities: Capabilities,
    /// What verifies a browser session token.
    pub sessions: Sessions,
    /// The object-store handles, read and upload, over one owner.
    ///
    /// Split inside [`ServingPlane::new`] rather than out here, because the two
    /// halves are one configuration decision: a deployment either set the R2
    /// knobs or did not, and handing over two independently-built values would
    /// let a caller pair a live reader with an absent writer.
    pub stores: Stores,
    /// The credential broker, built before the plane because it reads the
    /// vault, which is asynchronous where this constructor is not.
    pub broker: Arc<afd_credential::credential::Broker>,
    /// Where product events go, holding its own absence.
    ///
    /// Not an `Option`, for the reason [`PlaneParts::bundles`] is not: a
    /// deployment naming no `PostHog` project reports nothing, and a caller that
    /// had to ask before reporting is a caller that can forget.
    pub analytics: Analytics,
    /// The live-stream surface, holding its own absence.
    ///
    /// Not an `Option`, for the reason [`PlaneParts::bundles`] is not: an
    /// instance whose pub/sub connection could not be opened still SERVES the
    /// stream routes, silently, and `afd_sse::Live::detached` is that case as a
    /// value rather than as a `None` this file would unwrap into a refusal.
    /// Built before the plane because opening the hub is asynchronous where
    /// this constructor is not.
    pub live: Live,
    /// What the device-flow login surface needs from configuration.
    pub login: LoginConfig,
    /// The workspace holding this deployment's own platform secrets.
    ///
    /// `None` for a deployment that configured none. Threaded through rather
    /// than re-read, because `preflight` has already parsed and validated it
    /// and a second reader could disagree with the first.
    pub platform_admin_workspace: Option<Uuid7>,
    /// What a signup event from the identity provider is verified against.
    ///
    /// Threaded through rather than re-read, for the reason
    /// [`PlaneParts::platform_admin_workspace`] is: `preflight` has already
    /// resolved it and a second reader could disagree with the first. `None`
    /// refuses every delivery — see `preflight::IDENTITY_WEBHOOK_SECRET_KNOB`.
    pub identity_webhook_secret: Option<SecretBytes>,
    /// What the schedules surface and the fire ingress need from configuration.
    pub schedule: ScheduleConfig,
}

/// What the schedules surface needs from configuration.
///
/// A struct rather than four more positional parameters, and for a sharper
/// reason than length: `token` and the two signing keys are all opaque strings,
/// so two of them transposed would compile and fail only as a 401 from the
/// vendor that reads like a wrong credential.
#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    /// The client the management calls go out on.
    pub client: reqwest::Client,
    /// This deployment's bearer for the external scheduler.
    pub token: String,
    /// Where a fire is expected to arrive — see [`qstash::destination_url`].
    pub destination: String,
    /// Which scheduler deployment the management calls go to.
    ///
    /// Resolved at boot rather than defaulted in the client, so a deployment
    /// falling back to the vendor's US region is a visible decision in one
    /// place — see [`crate::preflight::QSTASH_URL_KNOB`].
    pub api_base: String,
    /// The scheduler's signing keys, when this deployment configured them.
    ///
    /// `None` is fail-closed: every fire is refused, because a daemon that
    /// cannot verify a callback must not act on one.
    pub keys: Option<SigningKeys>,
}

mod services;

impl Dependencies for ServingPlane {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        self.probes.probe()
    }
}

/// What the device-flow login surface needs from configuration.
///
/// A struct rather than two more positional parameters on a constructor that
/// already takes seven: a `SecretBytes` and a `String` next to each other are
/// two arguments a caller can transpose without the compiler noticing, and the
/// consequence would be a pepper rendered into every login URL.
#[derive(Debug, Clone)]
pub struct LoginConfig {
    /// The key a verification code's digest is taken under.
    pub code_pepper: SecretBytes,
    /// Where a person goes to approve a login.
    pub app_url: String,
    /// This deployment's own base URL, as a minted credential records it.
    ///
    /// Beside `app_url` because the two are read from configuration together
    /// and are the same kind of fact — where a person goes, and where this
    /// daemon answers. Never a request's `Host`: a credential and the
    /// deployment that minted it are one fact, and a client-asserted host
    /// would let them disagree.
    pub api_url: Box<str>,
}

/// The plane, ready to hand to the router.
///
/// `Arc` because axum clones the state per request and every field behind it is
/// a handle: cloning the plane itself would clone a pool handle, an entropy
/// selector and two registries per request, which is work with no product.
pub type Shared = Arc<ServingPlane>;

#[cfg(test)]
mod tests;
