//! The fluent configuration a suite reaches for after `Fleet::new`.
//!
//! Split from `fleet.rs` at the file cap, along the line between BUILDING an
//! instance and RECONFIGURING one. That file assembles the two a suite starts
//! from — one over datastores that answer nothing, one over live Postgres — and
//! every method here takes such an instance and swaps one seam in it. A second
//! inherent `impl Fleet` block is all that costs, and the two halves then grow
//! independently: a new dependency lands there, a new seam lands here.

#![expect(
    clippy::disallowed_methods,
    reason = "the harness's clients stand in for production's vendor, Slack and scheduler clients and reach only stubs or nothing; a proxy rule and a deadline are not what the router suites prove"
)]

use afd_connector::{Connectors, Exchange, Grants, PlatformApp};
use afd_cron::{Fire, QStash, ScheduleService, Schedules as CronSchedules};
use afd_ingress::Ingress;

use super::*;

impl Fleet {
    /// Uses the same wall-clock era as live database expiry checks.
    pub(crate) const fn at(mut self, now: jiff::Timestamp) -> Self {
        self.now = now;
        self
    }

    /// An instance that will carry `streams` at once and no more.
    pub(crate) fn carrying_at_most(mut self, streams: usize) -> Self {
        self.live = Live::detached(Ceiling::new(streams));
        self
    }

    /// A deployment that has set no active platform default.
    ///
    /// The reset's `UZ-PROVIDER-009` is the only refusal on this surface that
    /// turns on a row the harness cannot otherwise arrange: the default is
    /// global, so an integration lane sharing one database cannot assert its
    /// absence, and over the dead pool the read is a 503 rather than the
    /// `None` the refusal needs. Substituting the one answer reaches the arm
    /// through the real router — see [`super::HarnessProviders`].
    pub(crate) fn without_platform_default(mut self) -> Self {
        self.providers = self.providers.without_platform_default();
        self
    }

    /// Runs the device-flow service over a live, test-owned Dragonfly connection.
    ///
    /// Only the session service is replaced. The remaining queue-backed seams
    /// stay unreachable so a device-flow proof cannot accidentally broaden
    /// into a fleet, approval, or event integration test.
    pub(crate) fn with_session_queue(mut self, queue: Dragonfly) -> Self {
        self.logins = Logins::new(
            afd_dragonfly::SessionStore::new(queue),
            SecretBytes::new(FIXTURE_PEPPER.to_vec()),
            Entropy::new(),
            FIXTURE_APP_URL,
        );
        self
    }

    /// Runs approval decisions over the live queue paired with `database`.
    pub(crate) fn with_approval_queue(mut self, database: Db, queue: Dragonfly) -> Self {
        let ledger = super::fleet::admissions(&database, &queue);
        self.approvals = Inbox::new(database, queue, ledger);
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
    pub(crate) fn with_live_connectors(
        mut self,
        database: Db,
        queue: Dragonfly,
        vendor: String,
    ) -> Self {
        let kek = Arc::new(Kek::from_bytes(FIXTURE_KEK));
        self.connectors = Connectors::new(
            PlatformApp::new(SecretVault::new(
                database.clone(),
                Arc::clone(&kek),
                Entropy::new(),
            )),
            Grants::new(
                SecretVault::new(database.clone(), Arc::clone(&kek), Entropy::new())
                    .with_activity_streams(afd_dragonfly::FleetStreams::new(queue.clone())),
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

    /// Runs native setup and purge over a live queue and database.
    pub(crate) fn with_fleet_queue(self, database: Db, queue: Dragonfly) -> Self {
        self.with_fleet_queue_budgets(database, queue, afd_admission::Budgets::default())
    }

    /// Refuses real admissions under explicit small ceilings, without stubbing the ledger.
    pub(crate) fn with_fleet_queue_budgets(
        mut self,
        database: Db,
        queue: Dragonfly,
        budgets: afd_admission::Budgets,
    ) -> Self {
        let streams = afd_dragonfly::FleetStreams::new(queue.clone());
        self.secrets = self.secrets.with_activity_streams(streams.clone());
        self.memories = self.memories.with_activity_streams(streams);
        self.creations = afd_authoring::Creations::new(
            self.authoring_setup.clone(),
            afd_admission::Admissions::for_tests(database.clone(), queue).with_budgets(budgets),
        );
        self.fleets = Fleets::new(database);
        self
    }

    /// Runs the signed-ingress routes over a live database and queue.
    ///
    /// Resolve bindings and secrets through production stores, then commit the
    /// admission. Dragonfly receives the notification after acceptance.
    pub(crate) fn with_live_ingress(mut self, database: Db, queue: Dragonfly) -> Self {
        let kek = Arc::new(Kek::from_bytes(FIXTURE_KEK));
        let ledger = afd_admission::Admissions::for_tests(database.clone(), queue);
        self.ingress = HarnessIngress::Unreachable(Box::new(Ingress::new(
            database.clone(),
            SecretVault::new(database, kek, Entropy::new()),
            ledger,
        )));
        self
    }

    /// Admit a verified schedule fire to the live ledger and notify through Dragonfly.
    pub(crate) fn with_live_fire(mut self, database: Db, queue: Dragonfly) -> Self {
        let ledger = afd_admission::Admissions::for_tests(database.clone(), queue);
        self.schedules = SchedulePlane::new(
            ScheduleService::new(
                CronSchedules::new(database, Entropy::new()),
                QStash::new(
                    reqwest::Client::new(),
                    afd_crypto::secret::SecretString::new(String::new()),
                    SCHEDULE_DESTINATION.to_owned(),
                    SCHEDULE_API_BASE.to_owned(),
                ),
            ),
            Fire::new(ledger),
            Entropy::new(),
        );
        self
    }

    /// Admit fleet messages to the live ledger and send wake notifications.
    pub(crate) fn with_steering_queue(mut self, database: Db, queue: Dragonfly) -> Self {
        self.steering = Steer::new(afd_admission::Admissions::for_tests(database, queue));
        self
    }

    /// Runs stream handlers through a live shared subscription connection.
    pub(crate) fn with_live_hub(mut self, hub: afd_dragonfly::SubscriptionHub) -> Self {
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

    /// A lease plane whose memory verbs and mint answer `refusal`, for a case
    /// that proves how those routes render it.
    pub(crate) fn with_lease_refusal(mut self, refusal: fn() -> afd_fleet::Error) -> Self {
        self.leases = super::NoWork::refusing(refusal);
        self
    }

    /// A lease plane whose mint succeeds with `token`, for the case that
    /// proves the route writes it into the body and nowhere else.
    pub(crate) fn with_lease_minting(
        mut self,
        token: &'static str,
        expires_at: jiff::Timestamp,
    ) -> Self {
        self.leases = super::NoWork::minting(token, expires_at);
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

    /// The directory, for a suite that revokes between two requests.
    pub(crate) const fn directory(&self) -> &MockDirectory {
        &self.mock_directory
    }

    /// The capability source, for a suite that narrows a subject.
    pub(crate) const fn capabilities(&self) -> &MockCapabilities {
        &self.capabilities
    }

    /// The lease plane's poll log, shared with the router this fleet builds,
    /// so a case taken before `router` reads what the handler handed it.
    pub(crate) fn lease_plane(&self) -> super::NoWork {
        self.leases.clone()
    }

    /// The live fleet store, for a stream fixture that changes rows directly.
    pub(crate) fn fleet_store(&self) -> Fleets {
        self.fleets.clone()
    }

    /// The production router, over this instance.
    /// The writeback log, shared with the router this fleet builds.
    ///
    /// Taken BEFORE `router` consumes the fleet: the seam holds an `Arc`, so
    /// the handle a case keeps and the one the handler writes through are the
    /// same log.
    pub(crate) fn signup_writebacks(&self) -> super::RecordingWriteback {
        self.signup_writeback.clone()
    }

    /// A writeback seam the case chose — the one that refuses, when a case has
    /// to prove the delivery is still answered after the provider would not
    /// take the write.
    ///
    /// Taken as a value rather than a flag so the log a case reads back is the
    /// one it handed in: build the seam, keep a clone, pass the other here.
    pub(crate) fn with_signup_writeback(mut self, seam: super::RecordingWriteback) -> Self {
        self.signup_writeback = seam;
        self
    }

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
}
