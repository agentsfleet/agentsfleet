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

use afd_admission::Admissions;
use afd_api::Planes;
use afd_api::router::{Dependencies, ReadyInputs};
use afd_approval::IntegrationGrants;
use afd_billing::Accounts;
use afd_core::id::Uuid7;
use afd_credential::provider::Providers;
use afd_credential::vault::Vault;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::SecretBytes;
use afd_dragonfly::Doorbell;
use afd_events::History;
use afd_fleet::lease::{Leases, Plane};
use afd_gate::gate::Gates;
use afd_memory::Memories;
use afd_observability::Analytics;
use afd_runner::Runners;
use afd_sse::Live;
use afd_state::Credentials;

use crate::identity::{Capabilities, Sessions, SignupWriteback};
use crate::probes::LiveDependencies;

/// The authenticator this daemon serves both planes through.
///
/// Spelled once, because the three parameters appear in the state type, in the
/// associated type, and at construction — and a reader should have to reconcile
/// them in one place rather than three.
pub type Authenticator = Planes<Credentials, Capabilities, Sessions>;

/// Everything one request is served through, in production.
///
/// Grouped the way it is built: the request-path handles every verb touches
/// sit here, and each surface's stores sit in the group its builder returns.
#[derive(Debug)]
pub struct ServingPlane {
    authoring: surfaces::Authoring,
    tenant: stores::Tenant,
    admin: stores::Admin,
    activity: stores::Activity,
    outbound: surfaces::Outbound,
    login: surfaces::Login,
    probes: LiveDependencies,
    signup_writeback: SignupWriteback,
    authenticator: Authenticator,
    runners: Runners,
    leases: Plane,
    /// What a signup event from the identity provider is verified against.
    identity_webhook_secret: Option<SecretBytes>,
    platform_admin_workspace: Option<Uuid7>,
    live: Live,
    analytics: Analytics,
}

impl ServingPlane {
    /// Keeps a daemon integration lane's connector calls on its local vendor.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_connector_endpoint(mut self, endpoint: Option<&str>) -> Self {
        if let Some(endpoint) = endpoint {
            self.outbound.connectors = self.outbound.connectors.pointed_at(endpoint.to_owned());
        }
        self
    }

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
    /// The broker arrives built after boot resolves platform credentials.
    ///
    /// The KEK arrives already shared. `preflight` resolved and validated it
    /// before this point and refuses boot without one, so every store below
    /// that opens a sealed row takes the SAME key — which is Milestone
    /// Invariant 3 as an ownership fact rather than as a rule about who reads
    /// which variable.
    ///
    /// Two handles are built first because several groups take a share:
    ///
    /// - ONE provider store, read by two planes. The lease verb resolves which
    ///   key a run dials with; the tenant surface reads and writes the row that
    ///   decides it. A `Providers` is a pool handle and an `Arc` over the
    ///   process key, so a clone shares one connection set and one key —
    ///   building a second would mean two values that could be pointed at
    ///   different keys by a later edit.
    /// - ONE admissions ledger, shared by every producer. A second value would
    ///   be a second place a deployment could point at a different pool, and
    ///   the whole point of the ledger is that acceptance has one home. The
    ///   outbound group takes it by value, so it is built after every group
    ///   that clones it.
    ///
    /// The lease plane is last, because it takes the pool, the queue and the
    /// key by value.
    #[must_use]
    pub fn new(parts: PlaneParts) -> Self {
        let PlaneParts {
            database,
            identity_webhook_secret,
            queue,
            kek,
            capabilities,
            sessions,
            signup_writeback,
            broker,
            platform_admin_workspace,
            live,
            analytics,
            login,
            schedule,
            slack_api_base,
            vendor_client,
            authoring_transport,
        } = parts;
        let providers = Providers::new(database.clone(), Arc::clone(&kek), Entropy::new());
        let admissions = Admissions::new(database.clone(), queue.clone(), Entropy::new());
        // One biller for every money path: fleet leases and setup chat reserve
        // against the same holds and settle into the same ledger (billing §11).
        let biller = crate::billing::biller(database.clone());
        let drafts =
            afd_authoring::Drafts::new(database.clone(), Entropy::new(), Arc::clone(&biller))
                .with_analytics(analytics.clone());
        let authoring = surfaces::authoring(
            drafts,
            &queue,
            &kek,
            &providers,
            &admissions,
            authoring_transport,
        );
        let doorbell = listening_doorbell(&live);
        let admin = stores::admin(&database);
        let activity_streams = afd_dragonfly::FleetStreams::new(queue.clone());
        let tenant = stores::tenant(&database, &kek, &providers, &activity_streams);
        let activity = stores::activity(&database, &queue, &kek, &admissions);
        let outbound = surfaces::outbound(
            &database,
            &queue,
            &kek,
            surfaces::OutboundParts {
                schedule,
                admissions,
                vendor_client,
                slack_api_base,
            },
        );
        Self {
            authoring,
            tenant,
            admin,
            activity,
            outbound,
            login: surfaces::login(login, &queue),
            probes: LiveDependencies::new(database.clone(), queue.clone()),
            signup_writeback,
            authenticator: Planes::new(Credentials::new(database.clone()), capabilities, sessions),
            runners: Runners::new(database.clone(), Entropy::new()),
            leases: lease_plane(database, queue, kek, providers, doorbell, broker, biller),
            identity_webhook_secret,
            platform_admin_workspace,
            live,
            analytics,
        }
    }
}

mod config;
mod platform;
mod services;
mod stores;
mod surfaces;

pub use self::config::{LoginConfig, PlaneParts, ScheduleConfig};

/// The plane every runner verb acts through: leases, gates, grants, wallets,
/// memory, provider keys, the chat thread and declared credentials.
///
/// Takes the pool, the queue and the key by value, so [`ServingPlane::new`]
/// builds it after every other group has cloned what it needs.
fn lease_plane(
    database: afd_db::Db,
    queue: afd_dragonfly::Dragonfly,
    kek: Arc<afd_crypto::secret::Kek>,
    providers: Providers,
    doorbell: Doorbell,
    broker: Arc<afd_credential::credential::Broker>,
    biller: Arc<dyn afd_billing::biller::Biller>,
) -> Plane {
    let activity_streams = afd_dragonfly::FleetStreams::new(queue.clone());
    Plane {
        leases: Leases::new(database.clone(), queue.clone(), Entropy::new())
            .with_doorbell(doorbell),
        gates: Gates::new(database.clone(), queue, Entropy::new()),
        // The same surface the activity group's `grants` holds, over the
        // same table: a delivery that finds no grant asks through the crate
        // that owns the row rather than writing one.
        grants: IntegrationGrants::new(database.clone(), Entropy::new()),
        accounts: Accounts::new(database.clone()),
        biller,
        memories: Memories::new(database.clone(), Entropy::new())
            .with_activity_streams(activity_streams),
        providers,
        // The thread a chat lease reads its earlier turns from: the rows the
        // dashboard thread shows, through the same reader.
        thread: Arc::new(History::new(database.clone())),
        vault: Vault::new(database, kek),
        broker,
    }
}

/// The doorbell every poll the lease plane makes takes rung partitions from,
/// rung by this replica's hub. One per process: a second would hear rings no
/// poll reads. With no hub nothing rings, and polls find fresh work through
/// the rotation alone.
fn listening_doorbell(live: &Live) -> Doorbell {
    let doorbell = Doorbell::new();
    if let Some(hub) = live.hub() {
        doorbell.listen(hub);
    }
    doorbell
}

impl Dependencies for ServingPlane {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        self.probes.probe()
    }
}

/// The plane, ready to hand to the router.
///
/// `Arc` because axum clones the state per request and every field behind it is
/// a handle: cloning the plane itself would clone a pool handle, an entropy
/// selector and two registries per request, which is work with no product.
pub type Shared = Arc<ServingPlane>;

#[cfg(test)]
mod tests;
