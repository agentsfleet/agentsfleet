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

use afd_admin::{Models, PlatformKeys};
use afd_api::router::{Dependencies, ReadyInputs};
use afd_api::{Planes, Services};
use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_fleet::Runners;
use afd_fleet::bundle::Bundles;
use afd_fleet::gate::Gates;
use afd_fleet::lease::{Leases, Plane};
use afd_fleet::memory::Memories;
use afd_fleet::money::Accounts;
use afd_fleet::provider::Providers;
use afd_fleet::secrets::Registry;
use afd_fleet::streams::{LiveStreams, SSE_MAX_STREAMS_DEFAULT};
use afd_fleet::vault::Vault;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_library::{Libraries, LibraryImports};
use afd_redis::Redis;
use afd_state::Credentials;

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
    streams: LiveStreams,
    runner_lease_history: RunnerLeaseHistory,
    models: Models,
    platform_keys: PlatformKeys,
    libraries: Libraries,
    library_imports: LibraryImports,
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
    pub(crate) fn new(
        database: Db,
        queue: Redis,
        kek: Arc<Kek>,
        capabilities: Capabilities,
        sessions: Sessions,
        stores: Stores,
        broker: Arc<afd_fleet::credential::Broker>,
    ) -> Self {
        let (bundles, uploads) = stores.split();
        let library_imports = match uploads {
            Some(store) => LibraryImports::new(database.clone(), store),
            None => LibraryImports::without_store(database.clone()),
        };
        Self {
            bundles,
            streams: LiveStreams::new(SSE_MAX_STREAMS_DEFAULT),
            runner_lease_history: RunnerLeaseHistory::new(database.clone()),
            models: Models::new(database.clone(), Entropy::new()),
            platform_keys: PlatformKeys::new(database.clone()),
            libraries: Libraries::new(database.clone()),
            library_imports,
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

impl Dependencies for ServingPlane {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        self.probes.probe()
    }
}

impl Services for ServingPlane {
    type Auth = Authenticator;
    type Leases = Plane;

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

    fn streams(&self) -> &LiveStreams {
        &self.streams
    }

    fn runner_lease_history(&self) -> &RunnerLeaseHistory {
        &self.runner_lease_history
    }

    fn models(&self) -> &Models {
        &self.models
    }

    fn platform_keys(&self) -> &PlatformKeys {
        &self.platform_keys
    }

    fn libraries(&self) -> &Libraries {
        &self.libraries
    }

    fn library_imports(&self) -> &LibraryImports {
        &self.library_imports
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

/// The plane, ready to hand to the router.
///
/// `Arc` because axum clones the state per request and every field behind it is
/// a handle: cloning the plane itself would clone a pool handle, an entropy
/// selector and two registries per request, which is work with no product.
pub type Shared = Arc<ServingPlane>;
