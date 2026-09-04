//! Construction and fluent configuration for the HTTP test fleet.

use afd_connector::{Connectors, Exchange, Grants, PlatformApp};
use afd_cron::{Fire, QStash, ScheduleService, Schedules as CronSchedules};
use afd_ingress::Ingress;

use super::*;

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
        // Bound here rather than in the literal below: the fixture key is
        // MOVED into the last vault this constructor builds, and a store
        // declared after that point could not borrow it.
        let providers = HarnessProviders::Live(Providers::new(
            database.clone(),
            Arc::clone(&kek),
            Entropy::new(),
        ));
        Self {
            ready: ReadyInputs {
                database: true,
                queue: true,
            },
            mock_directory: mock,
            authenticator: Planes::new(
                directory.clone(),
                capabilities.clone(),
                MockVerifier::refusing(VerifyError::NotConfigured),
            ),
            directory,
            capabilities,
            runners: Runners::new(database.clone(), Entropy::new()),
            runner_lease_history: RunnerLeaseHistory::new(database.clone()),
            admin_models: AdminModels::new(database.clone(), Entropy::new()),
            platform_keys: PlatformKeys::new(database.clone()),
            libraries: Libraries::new(database.clone()),
            library_imports: LibraryImports::without_store(database.clone(), Entropy::new())
                .with_github_api_base(NOWHERE_GITHUB),
            leases: NoWork,
            // Unconfigured by default, so a suite that says nothing about
            // snapshots proves the refusal a deployment with no R2 knobs gives
            // — which is most of them.
            bundles: Bundles::unconfigured(),
            workspaces: OneWorkspace::fixed(),
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
            // The production connect flow, over stores that are not there and a
            // vendor nothing resolves. Same rule as every other seam: the
            // refusal a route gives is the one `afd_connector` raises, so the
            // whole matrix in front of these routes is reachable with no
            // datastore — which is the entire reason the seam is a trait.
            connectors: Connectors::new(
                PlatformApp::new(SecretVault::new(
                    database.clone(),
                    Arc::clone(&kek),
                    Entropy::new(),
                )),
                Grants::new(
                    SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
                    database.clone(),
                    Entropy::new(),
                ),
                Exchange::new(reqwest::Client::new()),
                reqwest::Client::new(),
                queue.clone(),
                Entropy::new(),
            ),
            // The production ingress, over stores that are not there. A suite
            // proving what happens PAST the first acquire swaps this arm out
            // with `Fleet::with_ingress`.
            signup_writeback: RecordingWriteback::accepting(),
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
                        SCHEDULE_API_BASE.to_owned(),
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
            signups: afd_tenant::signup::Signups::new(database.clone(), Entropy::new()),
            // Unconfigured by default, so a suite that says nothing about the
            // identity provider proves the fail-closed refusal a deployment
            // with no CLERK_WEBHOOK_SECRET gives.
            identity_webhook_secret: None,
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
            providers,
            catalogue: Models::new(database),
            dashboard_base: FIXTURE_APP_URL.to_owned(),
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
        // Bound here rather than in the literal below: the fixture key is
        // MOVED into the last vault this constructor builds, and a store
        // declared after that point could not borrow it.
        let providers = HarnessProviders::Live(Providers::new(
            database.clone(),
            Arc::clone(&kek),
            Entropy::new(),
        ));
        Self {
            ready: ReadyInputs {
                database: true,
                queue: true,
            },
            authenticator: Planes::new(
                directory.clone(),
                capabilities.clone(),
                MockVerifier::refusing(VerifyError::NotConfigured),
            ),
            mock_directory,
            directory,
            capabilities,
            runners: Runners::new(database.clone(), Entropy::new()),
            leases: NoWork,
            bundles: Bundles::unconfigured(),
            workspaces: OneWorkspace::fixed(),
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
            // The production connect flow, over stores that are not there and a
            // vendor nothing resolves. Same rule as every other seam: the
            // refusal a route gives is the one `afd_connector` raises, so the
            // whole matrix in front of these routes is reachable with no
            // datastore — which is the entire reason the seam is a trait.
            connectors: Connectors::new(
                PlatformApp::new(SecretVault::new(
                    database.clone(),
                    Arc::clone(&kek),
                    Entropy::new(),
                )),
                Grants::new(
                    SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
                    database.clone(),
                    Entropy::new(),
                ),
                Exchange::new(reqwest::Client::new()),
                reqwest::Client::new(),
                queue.clone(),
                Entropy::new(),
            ),
            // The production ingress, over stores that are not there. A suite
            // proving what happens PAST the first acquire swaps this arm out
            // with `Fleet::with_ingress`.
            signup_writeback: RecordingWriteback::accepting(),
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
                        SCHEDULE_API_BASE.to_owned(),
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
            signups: afd_tenant::signup::Signups::new(database.clone(), Entropy::new()),
            // Unconfigured by default, so a suite that says nothing about the
            // identity provider proves the fail-closed refusal a deployment
            // with no CLERK_WEBHOOK_SECRET gives.
            identity_webhook_secret: None,
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), queue.clone()),
            grants: IntegrationGrants::new(database.clone()),
            events: History::new(database.clone()),
            live: Live::detached(Ceiling::new(DEFAULT_STREAM_CEILING)),
            analytics: Analytics::silent(),
            steering: Steer::new(queue),
            memories: Memories::new(database.clone(), Entropy::new()),
            billing: Billing::new(database.clone()),
            providers,
            catalogue: Models::new(database.clone()),
            runner_lease_history: RunnerLeaseHistory::new(database.clone()),
            admin_models: AdminModels::new(database.clone(), Entropy::new()),
            platform_keys: PlatformKeys::new(database.clone()),
            libraries: Libraries::new(database.clone()),
            library_imports: LibraryImports::without_store(database, Entropy::new())
                .with_github_api_base(NOWHERE_GITHUB),
            dashboard_base: FIXTURE_APP_URL.to_owned(),
            now: UnixMillis::from_millis(FROZEN),
        }
    }
}
