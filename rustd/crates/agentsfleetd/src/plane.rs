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

use afd_api::router::{Dependencies, ReadyInputs};
use afd_api::{Planes, Services};
use afd_approval::Inbox;
use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_fleet::Runners;
use afd_fleet::bundle::Bundles;
use afd_fleet::gate::Gates;
use afd_fleet::lease::{Leases, Plane};
use afd_fleet::memory::Memories;
use afd_fleet::money::Accounts;
use afd_fleet::provider::Providers;
use afd_fleet::secrets::Registry;
use afd_fleet_lifecycle::Fleets;
use afd_tenant::preference::Preferences;
// Aliased: `crate::identity::Sessions` is the token VERIFIER, and this is the
// device-flow login surface. Two things called `Sessions` in one file is how a
// reader ends up believing the login surface verifies bearer tokens.
use afd_fleet::vault::Vault;
use afd_redis::Redis;
use afd_state::Credentials;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::billing::Billing;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
// Aliased: `afd_fleet::vault::Vault` above is the RUNNER plane's reader — it
// opens a credential a fleet declared and never lists — and this is the
// workspace-admin surface that seals, lists without a key, and deletes under
// the model-registry lock. Two things called `Vault` in one file is how a
// reader ends up believing one of them can do the other's job.
use afd_vault::Vault as SecretVault;

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
    secrets: SecretVault,
    preferences: Preferences,
    approvals: Inbox,
    api_url: Box<str>,
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
            queue,
            kek,
            capabilities,
            sessions,
            bundles,
            broker,
            login,
        } = parts;
        Self {
            bundles,
            workspaces: Workspaces::new(database.clone(), Entropy::new()),
            // Takes the Redis CONNECTION, not a view of it: which views the
            // fleet lifecycle needs is its own business, and assembling them
            // here would mean editing this file whenever that answer changed.
            fleets: Fleets::new(database.clone(), queue.clone(), Entropy::new()),
            api_keys: ApiKeys::new(database.clone(), Entropy::new()),
            cli_credentials: CliCredentials::new(database.clone(), Entropy::new()),
            billing: Billing::new(database.clone()),
            models: Models::new(database.clone()),
            // Takes the same shared key every other sealing store does, so a
            // row this daemon writes opens under the key the runner plane
            // reads it back with. `Arc::clone` and not a `Kek` clone: one copy
            // of the key material, zeroed once, however many stores hold it.
            secrets: SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), queue.clone()),
            api_url: login.api_url,
            logins: Logins::new(
                afd_redis::SessionStore::new(queue.clone()),
                login.code_pepper,
                Entropy::new(),
                &login.app_url,
            ),
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
    /// The Fleet Bundle snapshot store, possibly holding its own absence.
    ///
    /// Not an `Option`: `Bundles` carries the unconfigured case as a value that
    /// refuses with a registry code, so a deployment with no R2 knobs hands
    /// over `Bundles::unconfigured` rather than a `None` this file would unwrap
    /// into a refusal each handler re-invented.
    pub bundles: Bundles,
    /// The credential broker, built before the plane because it reads the
    /// vault, which is asynchronous where this constructor is not.
    pub broker: Arc<afd_fleet::credential::Broker>,
    /// What the device-flow login surface needs from configuration.
    pub login: LoginConfig,
}

impl Dependencies for ServingPlane {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        self.probes.probe()
    }
}

impl Services for ServingPlane {
    type Auth = Authenticator;
    type Leases = Plane;
    type Sessions = Logins;
    type Workspaces = Workspaces;
    type WorkspaceDirectory = Workspaces;
    type ApiKeys = ApiKeys;
    type CliCredentials = CliCredentials;
    type Fleets = Fleets;
    type Secrets = SecretVault;
    type Preferences = Preferences;
    type Approvals = Inbox;
    type Billing = Billing;
    type Catalogue = Models;

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn runners(&self) -> &Runners {
        &self.runners
    }

    fn leases(&self) -> &Plane {
        &self.leases
    }

    fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    fn sessions(&self) -> &Logins {
        &self.logins
    }

    fn workspaces(&self) -> &Workspaces {
        &self.workspaces
    }

    /// The same value as [`Services::workspaces`], deliberately: production
    /// holds one directory that answers both seams, and the split exists for
    /// the suites — see the trait's own note.
    fn workspace_directory(&self) -> &Workspaces {
        &self.workspaces
    }

    fn api_keys(&self) -> &ApiKeys {
        &self.api_keys
    }

    fn cli_credentials(&self) -> &CliCredentials {
        &self.cli_credentials
    }

    fn fleets(&self) -> &Fleets {
        &self.fleets
    }

    fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    fn approvals(&self) -> &Inbox {
        &self.approvals
    }

    fn secrets(&self) -> &SecretVault {
        &self.secrets
    }

    fn billing(&self) -> &Billing {
        &self.billing
    }

    fn catalogue(&self) -> &Models {
        &self.models
    }

    fn deployment(&self) -> &str {
        &self.api_url
    }

    /// The wall clock, read once per verb by whichever handler asked.
    ///
    /// Not a `Clock` behind an `Arc`: `afd_core::clock` reserves injection for
    /// an owner that reads repeatedly and asks everything else to take the
    /// instant as a parameter, which is exactly what a handler does with this.
    /// A test drives its own instant by implementing `Services` itself, so the
    /// seam a fixed clock would provide already exists one level up.
    fn now(&self) -> UnixMillis {
        afd_core::clock::now()
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
