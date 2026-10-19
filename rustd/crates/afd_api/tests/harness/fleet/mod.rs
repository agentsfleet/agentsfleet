//! Shared production-store setup for outage and live HTTP fixtures.

mod authoring;
mod clients;

use super::*;
use clients::{connectors, interjector, logins, schedules};

/// One durable ledger shared by this fixture's admission producers.
pub(crate) fn admissions(database: &Db, queue: &Dragonfly) -> afd_admission::Admissions {
    afd_admission::Admissions::for_tests(database.clone(), queue.clone())
}

impl Fleet {
    /// Uses real stores over unreachable dependencies for the refusal matrix.
    pub(crate) fn new() -> Self {
        Self::stores(Db::unreachable(&unreachable_pool()))
    }

    /// Uses the same store wiring with a live credential directory and database.
    pub(crate) fn live(database: Db, subject: &str, scopes: ScopeSet) -> Self {
        let who = Subject::new(subject).expect("the fixture subject is not blank");
        let mut fleet = Self::stores(database.clone());
        fleet.directory = Directory::Live(Credentials::new(database));
        fleet.capabilities = MockCapabilities::new().with(&who, scopes);
        fleet.authenticator = Planes::new(
            fleet.directory.clone(),
            fleet.capabilities.clone(),
            MockVerifier::refusing(VerifyError::NotConfigured),
        );
        fleet
    }

    /// Keeps real pub/sub while setting the stream ceiling for the load ladder.
    pub(crate) fn with_stream_capacity(
        mut self,
        hub: afd_dragonfly::SubscriptionHub,
        capacity: usize,
    ) -> Self {
        self.live = Live::new(hub, Ceiling::new(capacity));
        self
    }

    fn stores(database: Db) -> Self {
        let Inputs {
            database,
            queue,
            ledger,
            providers,
            mock_directory,
            directory,
            capabilities,
            authenticator,
        } = Inputs::new(database);
        let authoring = authoring::stores(&database, &queue, &ledger, &providers);
        Self {
            ready: ReadyInputs {
                database: true,
                queue: true,
            },
            mock_directory,
            directory,
            capabilities,
            authenticator,
            runners: Runners::new(database.clone(), Entropy::new()),
            runner_lease_history: RunnerLeaseHistory::new(database.clone()),
            admin_models: AdminModels::new(database.clone(), Entropy::new()),
            platform_keys: PlatformKeys::new(database.clone()),
            leases: NoWork::default(),
            authoring: authoring.drafts,
            authoring_setup: authoring.setup,
            authoring_models: authoring.models,
            creations: authoring.creations,
            workspaces: Ownership::Stub(OneWorkspace::fixed()),
            workspace_directory: Workspaces::new(database.clone(), Entropy::new()),
            api_keys: ApiKeys::new(database.clone(), Entropy::new()),
            team: HarnessTeam::new(database.clone()),
            invite_mail: afd_mail::InviteMailer::new(vault(database.clone())),
            cli_credentials: CliCredentials::new(database.clone(), Entropy::new()),
            logins: logins(&queue),
            fleets: clients::fleets(&database, &queue),
            secrets: vault(database.clone()),
            connectors: connectors(&database, &queue),
            interjector: interjector(&database),
            signup_writeback: RecordingWriteback::accepting(),
            ingress: clients::ingress(&database, &ledger),
            schedules: schedules(&database, &ledger),
            schedule_keys: None,
            platform_admin: None,
            signups: afd_tenant::signup::Signups::new(database.clone(), Entropy::new()),
            identity_webhook_secret: None,
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), queue.clone(), ledger.clone()),
            grants: IntegrationGrants::new(database.clone(), Entropy::new()),
            events: History::new(database.clone()),
            live: Live::detached(Ceiling::new(DEFAULT_STREAM_CEILING)),
            analytics: Analytics::silent(),
            steering: Steer::new(ledger),
            memories: Memories::new(database.clone(), Entropy::new()),
            billing: Billing::new(database.clone()),
            catalogue: Models::new(database),
            providers,
            dashboard_base: clients::dashboard(),
            now: FROZEN,
        }
    }
}

/// Process-local dependency handles, created once before field assembly.
struct Inputs {
    database: Db,
    queue: Dragonfly,
    ledger: afd_admission::Admissions,
    providers: HarnessProviders,
    mock_directory: MockDirectory,
    directory: Directory,
    capabilities: MockCapabilities,
    authenticator: Planes<Directory, MockCapabilities, MockVerifier>,
}

impl Inputs {
    fn new(database: Db) -> Self {
        let mock_directory = MockDirectory::new();
        let directory = Directory::Mock(mock_directory.clone());
        let capabilities = MockCapabilities::new();
        let authenticator = Planes::new(
            directory.clone(),
            capabilities.clone(),
            MockVerifier::refusing(VerifyError::NotConfigured),
        );
        let queue =
            Dragonfly::unreachable(&unreachable_queue()).expect("a lazy manager opens no socket");
        let ledger = admissions(&database, &queue);
        let providers = HarnessProviders::Live(Providers::new(
            database.clone(),
            Arc::new(Kek::from_bytes(FIXTURE_KEK)),
            Entropy::new(),
        ));
        Self {
            database,
            queue,
            ledger,
            providers,
            mock_directory,
            directory,
            capabilities,
            authenticator,
        }
    }
}
