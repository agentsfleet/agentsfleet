//! The two instances a suite starts from, and the fixture values they hold.
//!
//! Split from [`super`] on the line that file's own header draws between
//! BUILDING a fixture and ANSWERING for it. `mod.rs` declares what a [`Fleet`]
//! IS — its seams, and the arrangement verbs that file rows into them — and
//! this assembles the two it can be: one over datastores that answer nothing,
//! one over live Postgres. Everything here is construction, so a dependency
//! changing touches this file and a surface growing touches that one.

use std::sync::Arc;

use afd_api::router::{Dependencies, ReadyInputs, build};
use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT, Planes, Services};
use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialDirectory, CredentialRecord, Digest, Liveness};
use afd_auth::error::Unavailable;
use afd_auth::mock::{MockCapabilities, MockDirectory};
use afd_auth::principal::Subject;
use afd_auth::scope::ScopeSet;
use afd_auth::verifier::NoVerifier;
use afd_billing::tenant::Billing;
use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_events::{History, Steer};
use afd_fleet::bundle::{Bundles, ContentHash};
use afd_fleet::memory::Memories;
use afd_fleet_lifecycle::Fleets;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_library::{Libraries, LibraryImports};
use afd_observability::Analytics;
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use afd_runner::Runners;
use afd_sse::{Ceiling, Live};
use afd_state::Credentials;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
// Aliased for the reason the composition root aliases it: `afd_credential::vault`
// is the runner plane's reader and this is the workspace-admin surface.
use afd_admin::{Models as AdminModels, PlatformKeys};
use afd_approval::{Inbox, IntegrationGrants};
use afd_ingress::Ingress;
use afd_tenant::preference::Preferences;
use afd_vault::Vault as SecretVault;
use axum::Router;
use bytes::Bytes;
use object_store::ObjectStoreExt as _;
use object_store::memory::InMemory;

use super::readiness::{unreachable_pool, unreachable_queue};
use afd_api::SchedulePlane;
use afd_cron::{Fire, QStash, ScheduleService, Schedules as CronSchedules};

use super::SCHEDULE_DESTINATION;
use super::stubs_ingress::HarnessIngress;
use super::{Directory, Fleet, NoWork, OneWorkspace};

/// A fixed instant, so every row a verb writes is stamped predictably.
const FROZEN: i64 = 1_760_000_000_000;

/// The process key the secret store seals under.
///
/// Never used to seal anything here — every write refuses at the pool, before
/// an envelope is built — but a `Vault` cannot be CONSTRUCTED without one, which
/// is the invariant that type exists to carry. Supplying a fixture key is how a
/// suite honours it rather than working around it.
const FIXTURE_KEK: [u8; 32] = [0x11; 32];

/// The pepper the device-flow code digest is computed under, for the same reason.
const FIXTURE_PEPPER: &[u8] = b"fixture-session-code-pepper";

/// The dashboard origin a login surface composes approval links against.
const FIXTURE_APP_URL: &str = "https://app.fixture.test";

/// How many streams a fixture instance carries.
///
/// Above anything a suite opens, so a stream refused in a test is refused by
/// the thing that test is about. The one suite that DOES prove the ceiling
/// lowers it with [`Fleet::carrying_at_most`].
const DEFAULT_STREAM_CEILING: usize = 64;

impl Fleet {
    /// An instance whose dependencies answer, whose directory is empty, and
    /// whose Postgres and Redis are not there.
    ///
    /// Every store below is the PRODUCTION one. None of them is reachable, so
    /// every verb refuses at its first acquire — with the error its own crate
    /// raises, not one this file made up.
    pub(crate) fn new() -> Self {
        let mock = MockDirectory::new();
        let directory = Directory::Mock(mock.clone());
        let capabilities = MockCapabilities::new();
        let database = Db::unreachable(&unreachable_pool());
        let queue = Redis::unreachable(&unreachable_queue())
            .expect("a lazy manager opens no socket, so it cannot fail to open one");
        let kek = Arc::new(Kek::from_bytes(FIXTURE_KEK));
        Self {
            ready: ReadyInputs {
                database: true,
                queue: true,
            },
            mock_directory: mock,
            authenticator: Planes::new(directory.clone(), capabilities.clone(), NoVerifier),
            directory,
            capabilities,
            runners: Runners::new(database.clone(), Entropy::new()),
            runner_lease_history: RunnerLeaseHistory::new(database.clone()),
            admin_models: AdminModels::new(database.clone(), Entropy::new()),
            platform_keys: PlatformKeys::new(database.clone()),
            libraries: Libraries::new(database.clone()),
            library_imports: LibraryImports::without_store(database.clone()),
            leases: NoWork,
            // Unconfigured by default, so a suite that says nothing about
            // snapshots proves the refusal a deployment with no R2 knobs gives
            // — which is most of them.
            bundles: Bundles::unconfigured(),
            workspaces: OneWorkspace,
            workspace_directory: Workspaces::new(database.clone(), Entropy::new()),
            api_keys: ApiKeys::new(database.clone(), Entropy::new()),
            cli_credentials: CliCredentials::new(database.clone(), Entropy::new()),
            logins: Logins::new(
                afd_redis::SessionStore::new(queue.clone()),
                SecretBytes::new(FIXTURE_PEPPER.to_vec()),
                Entropy::new(),
                FIXTURE_APP_URL,
            ),
            fleets: Fleets::new(database.clone(), queue.clone(), Entropy::new()),
            secrets: SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
            // The production ingress, over stores that are not there. A suite
            // proving what happens PAST the first acquire swaps this arm out
            // with `Fleet::with_ingress`.
            ingress: HarnessIngress::Unreachable(Box::new(Ingress::new(
                database.clone(),
                SecretVault::new(database.clone(), kek, Entropy::new()),
                queue.clone(),
            ))),
            // The production schedules plane, over stores that are not there
            // and a scheduler nothing resolves. Same rule as every other seam:
            // the refusal a route gives is the one its own crate raises.
            schedules: SchedulePlane::new(
                ScheduleService::new(
                    CronSchedules::new(database.clone(), Entropy::new()),
                    QStash::new(
                        reqwest::Client::new(),
                        String::new(),
                        SCHEDULE_DESTINATION.to_owned(),
                    ),
                ),
                Fire::new(queue.clone()),
                Entropy::new(),
            ),
            // Fail-closed by default — a suite that proves a verified fire
            // arranges keys with `Fleet::with_schedule_keys`.
            schedule_keys: None,
            // No admin workspace, which is the fail-closed App-ingress state.
            platform_admin: None,
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), queue.clone()),
            grants: IntegrationGrants::new(database.clone()),
            events: History::new(database.clone()),
            // Detached, not connected: a hub opens a pub/sub SOCKET, which is
            // the one seam in this file that has no `unreachable` form. The
            // stream routes still answer and still charge the ceiling, which is
            // the whole of what a refusal-matrix suite reads.
            live: Live::detached(Ceiling::new(DEFAULT_STREAM_CEILING)),
            // Silent: a suite must not open a socket to a product-analytics
            // vendor, and every reporting call is infallible either way.
            analytics: Analytics::silent(),
            steering: Steer::new(queue.clone()),
            memories: Memories::new(database.clone(), Entropy::new()),
            billing: Billing::new(database.clone()),
            catalogue: Models::new(database),
            now: UnixMillis::from_millis(FROZEN),
        }
    }

    /// An instance whose credential directory and stores share live Postgres.
    ///
    /// The seam the admin and operator suites need: everything else in this
    /// file refuses at the first acquire, which proves a refusal matrix and
    /// nothing about a row. Redis stays unreachable — no suite built on this
    /// reaches a queue, and opening one would make a datastore lane out of a
    /// router lane.
    pub(crate) fn live(database: Db, subject: &str, scopes: ScopeSet) -> Self {
        let who = Subject::new(subject).expect("the fixture subject is not blank");
        let capabilities = MockCapabilities::new().with(&who, scopes);
        let mock_directory = MockDirectory::new();
        let directory = Directory::Live(Credentials::new(database.clone()));
        let queue = Redis::unreachable(&unreachable_queue())
            .expect("a lazy manager opens no socket, so it cannot fail to open one");
        let kek = Arc::new(Kek::from_bytes(FIXTURE_KEK));
        Self {
            ready: ReadyInputs {
                database: true,
                queue: true,
            },
            authenticator: Planes::new(directory.clone(), capabilities.clone(), NoVerifier),
            mock_directory,
            directory,
            capabilities,
            runners: Runners::new(database.clone(), Entropy::new()),
            leases: NoWork,
            bundles: Bundles::unconfigured(),
            workspaces: OneWorkspace,
            workspace_directory: Workspaces::new(database.clone(), Entropy::new()),
            api_keys: ApiKeys::new(database.clone(), Entropy::new()),
            cli_credentials: CliCredentials::new(database.clone(), Entropy::new()),
            logins: Logins::new(
                afd_redis::SessionStore::new(queue.clone()),
                SecretBytes::new(FIXTURE_PEPPER.to_vec()),
                Entropy::new(),
                FIXTURE_APP_URL,
            ),
            fleets: Fleets::new(database.clone(), queue.clone(), Entropy::new()),
            secrets: SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
            // The production ingress, over stores that are not there. A suite
            // proving what happens PAST the first acquire swaps this arm out
            // with `Fleet::with_ingress`.
            ingress: HarnessIngress::Unreachable(Box::new(Ingress::new(
                database.clone(),
                SecretVault::new(database.clone(), kek, Entropy::new()),
                queue.clone(),
            ))),
            // The production schedules plane, over stores that are not there
            // and a scheduler nothing resolves. Same rule as every other seam:
            // the refusal a route gives is the one its own crate raises.
            schedules: SchedulePlane::new(
                ScheduleService::new(
                    CronSchedules::new(database.clone(), Entropy::new()),
                    QStash::new(
                        reqwest::Client::new(),
                        String::new(),
                        SCHEDULE_DESTINATION.to_owned(),
                    ),
                ),
                Fire::new(queue.clone()),
                Entropy::new(),
            ),
            // Fail-closed by default — a suite that proves a verified fire
            // arranges keys with `Fleet::with_schedule_keys`.
            schedule_keys: None,
            // No admin workspace, which is the fail-closed App-ingress state.
            platform_admin: None,
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), queue.clone()),
            grants: IntegrationGrants::new(database.clone()),
            events: History::new(database.clone()),
            live: Live::detached(Ceiling::new(DEFAULT_STREAM_CEILING)),
            analytics: Analytics::silent(),
            steering: Steer::new(queue),
            memories: Memories::new(database.clone(), Entropy::new()),
            billing: Billing::new(database.clone()),
            catalogue: Models::new(database.clone()),
            runner_lease_history: RunnerLeaseHistory::new(database.clone()),
            admin_models: AdminModels::new(database.clone(), Entropy::new()),
            platform_keys: PlatformKeys::new(database.clone()),
            libraries: Libraries::new(database.clone()),
            library_imports: LibraryImports::without_store(database),
            now: UnixMillis::from_millis(FROZEN),
        }
    }

    /// An instance that will carry `streams` at once and no more.
    pub(crate) fn carrying_at_most(mut self, streams: usize) -> Self {
        self.live = Live::detached(Ceiling::new(streams));
        self
    }

    /// An instance reporting `ready` to `/readyz`.
    pub(crate) const fn reporting(mut self, ready: ReadyInputs) -> Self {
        self.ready = ready;
        self
    }
}
