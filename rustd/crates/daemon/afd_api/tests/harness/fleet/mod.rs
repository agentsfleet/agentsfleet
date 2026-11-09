//! Shared production-store setup for outage and live HTTP fixtures.

mod authoring;
mod clients;

use super::*;
use clients::{connectors, interjector, logins, schedules};

/// One durable ledger shared by this fixture's admission producers.
pub(crate) fn admissions(database: &Db, bus: &afd_bus::Handle) -> afd_admission::Admissions {
    afd_admission::Admissions::for_tests(database.clone(), bus.clone())
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

    /// Streams through `bus` while setting the stream ceiling for the load ladder.
    pub(crate) fn with_stream_capacity(mut self, bus: afd_bus::Handle, capacity: usize) -> Self {
        self.live = Live::new(bus, StreamLimit::new(capacity));
        self
    }

    fn stores(database: Db) -> Self {
        let Inputs {
            database,
            bus,
            ledger,
            providers,
            mock_directory,
            directory,
            capabilities,
            authenticator,
        } = Inputs::new(database);
        let authoring = authoring::stores(&database, &ledger, &providers);
        Self {
            native: None,
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
            tasks: tasks(&database, &bus, &ledger),
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
            logins: logins(&database),
            fleets: clients::fleets(&database),
            secrets: vault(database.clone()),
            connectors: connectors(&database),
            interjector: interjector(&database),
            signup_writeback: RecordingWriteback::accepting(),
            ingress: clients::ingress(&database, &ledger),
            schedules: schedules(&database, &ledger),
            schedule_keys: None,
            platform_admin: None,
            signups: afd_tenant::signup::Signups::new(database.clone(), Entropy::new()),
            identity_webhook_secret: None,
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), bus, ledger.clone()),
            grants: IntegrationGrants::new(database.clone(), Entropy::new()),
            events: History::new(database.clone()),
            live: Live::detached(StreamLimit::new(DEFAULT_STREAM_CEILING)),
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
    bus: afd_bus::Handle,
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
        let bus = afd_bus::test_util::unreachable();
        let ledger = admissions(&database, &bus);
        let providers = HarnessProviders::Live(Providers::new(
            database.clone(),
            Arc::new(Kek::from_bytes(FIXTURE_KEK)),
            Entropy::new(),
        ));
        Self {
            database,
            bus,
            ledger,
            providers,
            mock_directory,
            directory,
            capabilities,
            authenticator,
        }
    }
}

fn tasks(
    database: &Db,
    bus: &afd_bus::Handle,
    ledger: &afd_admission::Admissions,
) -> afd_fleet::task::Tasks {
    afd_fleet::task::Tasks::new(
        afd_fleet::lease::Leases::new(database.clone(), bus.clone(), Entropy::new()),
        ledger.clone(),
        task_dependencies(database.clone()),
    )
}
