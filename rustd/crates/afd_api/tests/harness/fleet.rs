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
            library_imports: LibraryImports::without_store(database.clone()),
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

    /// Authorizes a minted workspace rather than the datastore-free fixture id.
    pub(crate) fn with_owned_workspace(mut self, workspace: Uuid7) -> Self {
        self.workspaces = OneWorkspace::owning(workspace);
        self
    }

    /// Runs the device-flow service over a live, test-owned Redis connection.
    ///
    /// Only the session service is replaced. The remaining queue-backed seams
    /// stay unreachable so a device-flow proof cannot accidentally broaden
    /// into a fleet, approval, or event integration test.
    pub(crate) fn with_session_queue(mut self, queue: Redis) -> Self {
        self.logins = Logins::new(
            afd_redis::SessionStore::new(queue),
            SecretBytes::new(FIXTURE_PEPPER.to_vec()),
            Entropy::new(),
            FIXTURE_APP_URL,
        );
        self
    }

    /// Runs approval decisions over the live queue paired with `database`.
    pub(crate) fn with_approval_queue(mut self, database: Db, queue: Redis) -> Self {
        self.approvals = Inbox::new(database, queue);
        self
    }

    /// Runs the connect flow over live stores and a vendor a test can answer.
    ///
    /// Three seams move together and none of them is optional. The nonce slot
    /// lives in the QUEUE, so a spend against an unreachable one proves nothing
    /// about single use. The grant is sealed in the VAULT, so its landing is
    /// only observable over a live pool. And the token exchange posts to the
    /// provider's own endpoint, which no test may reach — `Exchange::pointed_at`
    /// is the seam the crate already carries for exactly this, and pointing it
    /// at a loopback server is what makes a completed connect reachable at all.
    pub(crate) fn with_live_connectors(mut self, database: Db, queue: Redis, vendor: String) -> Self {
        let kek = Arc::new(Kek::from_bytes(FIXTURE_KEK));
        self.connectors = Connectors::new(
            PlatformApp::new(SecretVault::new(
                database.clone(),
                Arc::clone(&kek),
                Entropy::new(),
            )),
            Grants::new(
                SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new()),
                database,
                Entropy::new(),
            ),
            Exchange::new(reqwest::Client::new()).pointed_at(vendor),
            reqwest::Client::new(),
            queue,
            Entropy::new(),
        );
        self
    }

    /// Runs fleet installation and purge over a live queue and database.
    pub(crate) fn with_fleet_queue(mut self, database: Db, queue: Redis) -> Self {
        self.fleets = Fleets::new(database, queue, Entropy::new());
        self
    }

    /// Runs the fleet message ingress over a live queue.
    pub(crate) fn with_steering_queue(mut self, queue: Redis) -> Self {
        self.steering = Steer::new(queue);
        self
    }

    /// Runs stream handlers through a live shared subscription connection.
    pub(crate) fn with_live_hub(mut self, hub: afd_redis::SubscriptionHub) -> Self {
        self.live = Live::new(hub, Ceiling::new(DEFAULT_STREAM_CEILING));
        self
    }

    /// An instance reporting `ready` to `/readyz`.
    pub(crate) const fn reporting(mut self, ready: ReadyInputs) -> Self {
        self.ready = ready;
        self
    }

    /// Files a runner row under the digest of `token`.
    pub(crate) fn with_runner(self, token: &str, runner: &Uuid7, live: Liveness) -> Self {
        file_runner(&self.mock_directory, token, runner, live);
        self
    }

    /// Files a person row under the digest of `key`, holding `scopes`.
    pub(crate) fn with_person(mut self, key: &str, subject: &str, scopes: ScopeSet) -> Self {
        let who = Subject::new(subject).expect("the fixture subject is not blank");
        let _filed = self.mock_directory.clone().with(
            CredentialKind::TenantApiKey,
            &presented(key),
            CredentialRecord::Person {
                tenant: tenant(),
                subject: who.clone(),
                live: Liveness::Live,
            },
        );
        self.capabilities = self.capabilities.with(&who, scopes);
        self
    }

    /// Files a person row under the digest of an `afc_` command-line credential.
    ///
    /// Sibling of [`Self::with_person`], and the difference is the whole point:
    /// the two resolve to the same person with the same capabilities and differ
    /// only in credential CLASS, which is exactly the axis the command-line
    /// credential routes refuse on. A suite cannot prove that rule with one of
    /// them.
    pub(crate) fn with_terminal(
        mut self,
        credential: &str,
        subject: &str,
        scopes: ScopeSet,
    ) -> Self {
        let who = Subject::new(subject).expect("the fixture subject is not blank");
        self.mock_directory = self.mock_directory.with(
            CredentialKind::CliCredential,
            &presented(credential),
            CredentialRecord::Person {
                tenant: tenant(),
                subject: who.clone(),
                live: Liveness::Live,
            },
        );
        self.capabilities = self.capabilities.with(&who, scopes);
        self
    }

    /// Accepts an unmarked bearer as a browser dashboard session.
    ///
    /// Session tokens do not use the credential directory: their verified
    /// claims are the identity. Rebuilding the two plane registries here keeps
    /// the fixture on the production authentication path while replacing only
    /// the key-set verifier that would otherwise need network access.
    pub(crate) fn with_dashboard(mut self, subject: &str) -> Self {
        use afd_auth::verifier::VerifiedClaims;

        let subject = Subject::new(subject).expect("the fixture subject is not blank");
        let claims = VerifiedClaims {
            subject: subject.clone(),
            tenant: Some(tenant()),
            workspace_scope: None,
            scope_claim: None,
        };
        self.capabilities = self.capabilities.with(&subject, ScopeSet::EMPTY);
        self.authenticator = Planes::new(
            self.directory.clone(),
            self.capabilities.clone(),
            MockVerifier::accepting(claims),
        );
        self
    }

    /// Backs this instance with an in-memory snapshot store holding `body`
    /// under `content_hash`.
    ///
    /// `object_store::memory::InMemory` rather than a mock of our own: it is
    /// the backend the workspace manifest names for exactly this, so what the
    /// suite drives is the same client production drives with a different
    /// backing store — not a second implementation that could agree with the
    /// test and disagree with R2.
    ///
    /// Async because a `put` is, which is why it is not one of the `const`
    /// builders above.
    pub(crate) async fn with_snapshot(mut self, content_hash: &str, body: &[u8]) -> Self {
        let store = InMemory::new();
        let hash = ContentHash::parse(content_hash).expect("the fixture digest is well formed");
        store
            .put(&hash.snapshot_key(), Bytes::copy_from_slice(body).into())
            .await
            .expect("an in-memory put cannot fail");
        self.bundles = Bundles::new(Arc::new(store));
        self
    }

    /// The directory, for a suite that revokes between two requests.
    pub(crate) const fn directory(&self) -> &MockDirectory {
        &self.mock_directory
    }

    /// The capability source, for a suite that narrows a subject.
    pub(crate) const fn capabilities(&self) -> &MockCapabilities {
        &self.capabilities
    }

    /// A handle a long-lived stream test can revoke after the router opens.
    pub(crate) fn ownership(&self) -> OneWorkspace {
        self.workspaces.clone()
    }

    /// The live fleet store, for a stream fixture that changes rows directly.
    pub(crate) fn fleet_store(&self) -> Fleets {
        self.fleets.clone()
    }

    /// The production router, over this instance.
    pub(crate) fn router(self) -> Router {
        let admission = Admission::new(DEFAULT_MAX_IN_FLIGHT);
        build(Arc::new(self), &admission)
    }

    /// An instance whose ingress ANSWERS, rather than refusing at an acquire.
    ///
    /// The one seam a signed-ingress suite has to arrange: every store in this
    /// harness is the production one over a datastore that is not there, which
    /// proves what these routes refuse and nothing about what they do once a
    /// delivery is believed. See [`stubs_ingress`] on why that arm exists and
    /// what it deliberately does not stand in for.
    pub(crate) fn with_ingress(mut self, scripted: &Arc<Scripted>) -> Self {
        self.ingress = HarnessIngress::Scripted(Arc::clone(scripted));
        self
    }

    /// An instance holding the scheduler's signing keys.
    ///
    /// Absent by default, which is the fail-closed state a fire is refused in.
    pub(crate) fn with_schedule_keys(mut self, current: &str, next: &str) -> Self {
        self.schedule_keys = Some(afd_cron::SigningKeys {
            current: current.to_owned(),
            next: next.to_owned(),
        });
        self
    }

    /// An instance that configured a platform admin workspace.
    ///
    /// `None` is the default and it is a real deployment state rather than an
    /// unset fixture: an App signs every installation's deliveries with ONE
    /// secret belonging to the deployment, so a daemon that was given no admin
    /// workspace has nowhere to read it from and fails closed. Leaving the
    /// default alone is how a suite reaches that branch.
    pub(crate) fn with_platform_admin(mut self, workspace: Uuid7) -> Self {
        self.platform_admin = Some(workspace);
        self
    }

    /// Configures the secret a signup event is verified against.
    ///
    /// `None` is the default and it is a real deployment state rather than an
    /// unset fixture: a daemon given no secret refuses every delivery, because
    /// accepting an unverified one on a route that CREATES ACCOUNTS is worse
    /// than serving none. Leaving the default alone is how a suite reaches
    /// that branch.
    pub(crate) fn with_identity_secret(mut self, secret: &str) -> Self {
        self.identity_webhook_secret = Some(afd_crypto::secret::SecretBytes::new(
            secret.as_bytes().to_vec(),
        ));
        self
    }
}
