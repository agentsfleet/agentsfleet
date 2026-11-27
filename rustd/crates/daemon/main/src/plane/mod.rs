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

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::SecretBytes;
use af_observability::Analytics;
use afd_api::router::{Dependencies, ReadyInputs};
use afd_approval::IntegrationGrants;
use afd_billing::Accounts;
use afd_credential::provider::Providers;
use afd_fleet::run::history::Conversations;
use afd_fleet::run::{RunService, Runs};
use afd_http::auth::Planes;
use afd_memory::Memories;
use afd_runner::Runners;
use afd_sse::Live;
use afd_state::Credentials;
use afd_turns::Turns;
use afd_vault::Vault;

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
    creations: afd_setup::Creations,
    tenant: stores::Tenant,
    admin: stores::Admin,
    activity: stores::Activity,
    outbound: surfaces::Outbound,
    login: surfaces::Login,
    probes: LiveDependencies,
    signup_writeback: SignupWriteback,
    authenticator: Authenticator,
    runners: Runners,
    runs: RunService,
    runner_streams: afd_gateway::Gateway,
    runner_commands: afd_bus::Handle,
    tasks: afd_fleet::durable_task::DurableTasks,
    /// What a signup event from the identity provider is verified against.
    identity_webhook_secret: Option<SecretBytes>,
    platform_admin_workspace: Option<Uuid7>,
    live: Live,
    analytics: Analytics,
    /// The page sizes every list route is bounded by (`[api]`).
    pages: afd_http::paging::Pages,
}

impl ServingPlane {
    /// One native session owner shared by the router and supervised runtime.
    #[must_use]
    pub fn runner_streams(&self) -> &afd_gateway::Gateway {
        &self.runner_streams
    }

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
    /// - ONE provider store, read by two planes. The run verb resolves which
    ///   key a run dials with; the tenant surface reads and writes the row that
    ///   decides it. A `Providers` is a pool handle and an `Arc` over the
    ///   process key, so a clone shares one connection set and one key —
    ///   building a second would mean two values that could be pointed at
    ///   different keys by a later edit.
    /// - ONE turns ledger, shared by every producer. A second value would
    ///   be a second place a deployment could point at a different pool, and
    ///   the whole point of the ledger is that acceptance has one home. The
    ///   outbound group takes it by value, so it is built after every group
    ///   that clones it.
    ///
    /// The run plane is last, because it takes the pool and the key by
    /// value.
    #[must_use]
    pub fn new(parts: PlaneParts) -> Self {
        let common = construction::Common::new(&parts);
        let tuning = parts.runtime.operations.tuning;
        let database = parts.database;
        let kek = parts.kek;
        let admin = stores::admin(&database);
        let activity_streams = parts.bus.clone();
        let tenant = stores::tenant(
            &database,
            &kek,
            &common.providers,
            &common.turns.notices(),
            crate::config::tuning::seconds(tuning.mail.send_timeout_seconds),
        );
        let activity = stores::activity(&database, &kek, &common.turns);
        let durable_task_turns = common.turns.clone();
        let outbound = surfaces::outbound(
            &database,
            &kek,
            surfaces::OutboundParts {
                schedule: parts.schedule,
                admin: parts.platform_admin_workspace.clone(),
                turns: common.turns,
                vendor_client: parts.vendor_client,
                slack_api_base: parts.slack_api_base,
                tuning,
            },
        );
        let native = construction::NativeInputs {
            runtime: parts.runtime,
            database: &database,
            kek,
            providers: common.providers,
            bus: parts.bus.clone(),
            broker: parts.broker,
            biller: common.biller,
            wake: parts.runner_wake,
            turns: durable_task_turns,
            admin_workspace: parts.platform_admin_workspace.clone(),
            storage_config: parts.checkpoint_storage,
            client: common.github_client,
            stream_limit: parts.live.stream_limit().clone(),
            frame_memory: parts.frame_memory,
            analytics: parts.analytics.clone(),
        }
        .build();
        Self {
            creations: common.creations,
            tenant,
            admin,
            activity,
            outbound,
            login: surfaces::login(parts.login, &database, &tuning.auth),
            probes: LiveDependencies::new(database.clone(), parts.bus),
            signup_writeback: parts.signup_writeback,
            authenticator: authenticator(database, parts.capabilities, parts.sessions),
            runners: native.runners,
            runner_streams: native.streams,
            runner_commands: activity_streams,
            runs: native.runs,
            tasks: native.tasks,
            identity_webhook_secret: parts.identity_webhook_secret,
            platform_admin_workspace: parts.platform_admin_workspace,
            live: parts.live,
            analytics: parts.analytics,
            pages: construction::pages(&tuning.api),
        }
    }
}

fn authenticator(
    database: afd_db::Db,
    capabilities: Capabilities,
    sessions: Sessions,
) -> Authenticator {
    Planes::new(Credentials::new(database), capabilities, sessions)
}

mod config;
mod construction;
mod platform;
mod services;
mod stores;
mod surfaces;

pub use self::config::{LoginConfig, PlaneParts, ScheduleConfig};

/// The plane every runner verb acts through: runs, gates, grants, wallets,
/// memory, provider keys, the chat thread and declared credentials.
///
/// Takes the pool and the key by value, so [`ServingPlane::new`]
/// builds it after every other group has cloned what it needs.
fn run_plane(
    database: afd_db::Db,
    Sealing { kek, cipher }: Sealing,
    providers: Providers,
    bus: afd_bus::Handle,
    broker: Arc<afd_credential::credential::Broker>,
    biller: Arc<dyn afd_billing::biller::Biller>,
    notices: afd_turns::view::activity::Notices,
) -> RunService {
    let vault = Vault::new(database.clone(), kek, Entropy::new());
    // Built once: every checkpoint path shares these keys, and this is where a
    // deployment that seals nothing says so.
    let keys = afd_fleet::durable_task::storage::CheckpointKeys::new(
        database.clone(),
        vault.clone(),
        cipher,
    );
    RunService {
        durable_task_reports: None,
        turn_checkpoints: afd_fleet::run::turn_checkpoint::TurnCheckpoints::new(keys),
        runs: Runs::new(database.clone(), bus, Entropy::new()),
        // The same surface the activity group's `grants` holds, over the
        // same table: a delivery that finds no grant asks through the crate
        // that owns the row rather than writing one.
        grants: IntegrationGrants::new(database.clone(), Entropy::new()),
        accounts: Accounts::new(database.clone()),
        biller,
        memories: Memories::new(database.clone(), Entropy::new()).with_activity_notices(notices),
        providers,
        // The thread a chat run reads its earlier turns and their tool
        // calls from.
        thread: Arc::new(Conversations::new(database)),
        vault,
        broker,
    }
}

/// What seals stored state: the vault's master key, and the checkpoint cipher
/// `[checkpoint] cipher` names.
struct Sealing {
    kek: Arc<af_crypto::secret::Kek>,
    cipher: af_wire::durable_task::CheckpointCipher,
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

/// One native task owner attaches reporting to the existing run plane.
fn durable_task_plane(
    runs: &mut RunService,
    turns: Turns,
    admin_workspace: Option<Uuid7>,
    storage_config: Option<garde::Valid<afd_fleet::durable_task::storage::CheckpointStorageConfig>>,
    client: reqwest::Client,
    policy: afd_fleet::durable_task::policy::DurableTaskPolicy,
) -> afd_fleet::durable_task::DurableTasks {
    let storage = storage_config
        .zip(admin_workspace.clone())
        .map(|(config, admin)| {
            afd_fleet::durable_task::storage::R2CheckpointStore::new(
                config,
                admin,
                runs.turn_checkpoints.keys().clone(),
                client.clone(),
            )
        });
    runs.turn_checkpoints = runs.turn_checkpoints.clone().with_storage(storage.clone());
    let tasks = afd_fleet::durable_task::DurableTasks::new(
        runs.runs.clone(),
        turns,
        afd_fleet::durable_task::DurableTaskDependencies {
            policy,
            admin_workspace,
            storage,
            github: Some(afd_fleet::durable_task::Github::new(runs.clone(), client)),
            ..afd_fleet::durable_task::DurableTaskDependencies::from(&*runs)
        },
    );
    runs.durable_task_reports = Some(Arc::new(tasks.clone()));
    tasks
}
