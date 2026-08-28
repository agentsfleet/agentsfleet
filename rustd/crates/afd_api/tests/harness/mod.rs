//! A production router over seams a test drives, and no datastore anywhere.
//!
//! The exonum testkit pattern the milestone names: real HTTP through the real
//! router, over the real layers, with the things that reach a network replaced
//! at their declared seams. What that buys is that the refusal matrix is proven
//! against the thing that ships — including the layer ORDER, which a test
//! calling a handler directly cannot see at all.
//!
//! # Every store is the REAL one, over datastores that answer nothing
//!
//! There is no seam between a store and its statements, deliberately: the SQL
//! is the parity target, so a fake store would prove a handler against SQL
//! nobody runs. Each store here therefore holds a pool over an address that
//! answers nothing ([`afd_db::Db::unreachable`]) and, where it needs one, a
//! queue built the same way ([`afd_redis::Redis::unreachable`]). That is
//! exactly what a datastore outage looks like from the request path, and it
//! lets a suite prove the transport-class refusal (RULE ECL) without stopping a
//! container.
//!
//! This replaced eight hand-written `No*` stubs, one per service seam, each
//! spelling `Err(Error::datastore_unavailable())` in every method. They were
//! not wrong, they were REDUNDANT — and worse than redundant in one specific
//! way: a stub that INVENTS the refusal keeps agreeing with the suite after the
//! real store stops producing it. Deleting them also deleted the
//! `test-util` constructors that existed only to fabricate that error.
//!
//! Two stubs survive, and neither is a uniform refusal — which is the line:
//! [`OneWorkspace`] answers ownership honestly so both halves of that matrix
//! stay reachable, and [`NoWork`] answers a lease plane's verbs with success so
//! the runner routes are reachable at all. Each carries test LOGIC no real
//! store over a dead datastore could stand in for.
//!
//! A test that needs rows is an integration test, `#[ignore]`d and run by
//! `make test-integration-rustd`. This harness is for everything BEFORE the
//! first row is read.
#![allow(
    dead_code,
    unused_imports,
    reason = "shared across suites; each uses the subset its dimensions need"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_api::router::{Dependencies, ReadyInputs, build};
use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT, Planes, Services};
use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::mock::{MockCapabilities, MockDirectory};
use afd_auth::principal::Subject;
use afd_auth::scope::ScopeSet;
use afd_auth::verifier::NoVerifier;
use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_fleet::bundle::{Bundles, ContentHash};
use afd_fleet_lifecycle::Fleets;
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use afd_runner::Runners;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::billing::Billing;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
// Aliased for the reason the composition root aliases it: `afd_fleet::vault`
// is the runner plane's reader and this is the workspace-admin surface.
use afd_approval::Inbox;
use afd_tenant::preference::Preferences;
use afd_vault::Vault as SecretVault;
use axum::Router;
use bytes::Bytes;
use object_store::ObjectStoreExt as _;
use object_store::memory::InMemory;

mod stubs_runner;
mod stubs_tenant;
mod support;

pub(crate) use self::stubs_runner::NoWork;
pub(crate) use self::stubs_tenant::{DEPLOYMENT, OWNED_WORKSPACE, OneWorkspace};
pub(crate) use self::support::{
    file_runner, json_body, presented, runner_id, send, send_with_headers, tenant,
};

/// A Postgres nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so an
/// acquire fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// acquire budgets.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// A Redis nobody is listening on, for the same reason and on the same port.
const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

/// The pool knob naming how long an acquire may spend before it reports.
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

/// What this harness sets it to — see [`unreachable_pool`].
const ACQUIRE_TIMEOUT_MS: &str = "50";

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

/// The seams a suite arranges, and the state the router is built over.
#[derive(Debug)]
pub(crate) struct Fleet {
    ready: ReadyInputs,
    directory: MockDirectory,
    capabilities: MockCapabilities,
    authenticator: Planes<MockDirectory, MockCapabilities, NoVerifier>,
    runners: Runners,
    leases: NoWork,
    bundles: Bundles,
    workspaces: OneWorkspace,
    workspace_directory: Workspaces,
    api_keys: ApiKeys,
    cli_credentials: CliCredentials,
    logins: Logins,
    fleets: Fleets,
    secrets: SecretVault,
    preferences: Preferences,
    approvals: Inbox,
    billing: Billing,
    catalogue: Models,
    now: UnixMillis,
}

impl Fleet {
    /// An instance whose dependencies answer, whose directory is empty, and
    /// whose Postgres and Redis are not there.
    ///
    /// Every store below is the PRODUCTION one. None of them is reachable, so
    /// every verb refuses at its first acquire — with the error its own crate
    /// raises, not one this file made up.
    pub(crate) fn new() -> Self {
        let directory = MockDirectory::new();
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
            authenticator: Planes::new(directory.clone(), capabilities.clone(), NoVerifier),
            directory,
            capabilities,
            runners: Runners::new(database.clone(), Entropy::new()),
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
            secrets: SecretVault::new(database.clone(), kek, Entropy::new()),
            preferences: Preferences::new(database.clone(), Entropy::new()),
            approvals: Inbox::new(database.clone(), queue.clone()),
            billing: Billing::new(database.clone()),
            catalogue: Models::new(database),
            now: UnixMillis::from_millis(FROZEN),
        }
    }

    /// An instance reporting `ready` to `/readyz`.
    pub(crate) const fn reporting(mut self, ready: ReadyInputs) -> Self {
        self.ready = ready;
        self
    }

    /// Files a runner row under the digest of `token`.
    pub(crate) fn with_runner(self, token: &str, runner: &Uuid7, live: Liveness) -> Self {
        file_runner(&self.directory, token, runner, live);
        self
    }

    /// Files a person row under the digest of `key`, holding `scopes`.
    pub(crate) fn with_person(mut self, key: &str, subject: &str, scopes: ScopeSet) -> Self {
        let who = Subject::new(subject).expect("the fixture subject is not blank");
        self.directory = self.directory.with(
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
        self.directory = self.directory.with(
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
        &self.directory
    }

    /// The capability source, for a suite that narrows a subject.
    pub(crate) const fn capabilities(&self) -> &MockCapabilities {
        &self.capabilities
    }

    /// The production router, over this instance.
    pub(crate) fn router(self) -> Router {
        let admission = Admission::new(DEFAULT_MAX_IN_FLIGHT);
        build(Arc::new(self), &admission)
    }
}

impl Dependencies for Fleet {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        std::future::ready(self.ready)
    }
}

impl Services for Fleet {
    type Auth = Planes<MockDirectory, MockCapabilities, NoVerifier>;
    type Leases = NoWork;
    type Sessions = Logins;
    type Workspaces = OneWorkspace;
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

    fn leases(&self) -> &NoWork {
        &self.leases
    }

    fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    fn sessions(&self) -> &Logins {
        &self.logins
    }

    fn workspaces(&self) -> &OneWorkspace {
        &self.workspaces
    }

    /// A different value from [`Services::workspaces`], unlike production.
    ///
    /// The split is the suites': the ownership seam has to answer HONESTLY for
    /// both halves of the refusal matrix to be reachable, so it stays
    /// [`OneWorkspace`], while the directory refuses like every other store.
    fn workspace_directory(&self) -> &Workspaces {
        &self.workspace_directory
    }

    fn api_keys(&self) -> &ApiKeys {
        &self.api_keys
    }

    fn cli_credentials(&self) -> &CliCredentials {
        &self.cli_credentials
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

    fn fleets(&self) -> &Fleets {
        &self.fleets
    }

    fn billing(&self) -> &Billing {
        &self.billing
    }

    fn catalogue(&self) -> &Models {
        &self.catalogue
    }

    /// A fixed deployment, which is what a real one is too.
    ///
    /// Read from configuration in the binary rather than from the request, so a
    /// constant here is the same KIND of value the daemon serves with — not a
    /// simplification a suite would have to remember is one.
    fn deployment(&self) -> &str {
        DEPLOYMENT
    }

    fn now(&self) -> UnixMillis {
        self.now
    }
}

/// A Postgres configuration pointed at an address that answers nothing.
///
/// Port 1 is reserved and unbound on every platform this builds for, so an
/// acquire fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// acquire budgets.
fn unreachable_pool() -> PoolConfig {
    let environment = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), NOWHERE),
        // The acquire budget, cut from the two-second production default.
        //
        // Every request in this harness ends at a refused connection, and the
        // pool spends the whole budget retrying before it reports one. At the
        // default that is two seconds per request and roughly ten per suite —
        // paid on every inner-loop run, to learn something the first
        // millisecond already knew.
        //
        // Set through the SAME knob a deployment sets, not through a test-only
        // constructor, so what the suite configures is what an operator can. It
        // must not go so low that the pool gives up before its first connect
        // attempt returns: `sqlx` reports that as `PoolTimedOut`, which
        // `afd_db` classifies as pool CAPACITY rather than an unreachable
        // datastore, and the refusal would change class. A refused TCP connect
        // on a reserved port answers in microseconds, so this has three orders
        // of magnitude of headroom — and if it ever stops having them, the
        // assertions on `DATABASE_UNAVAILABLE` fail loudly rather than drifting.
        (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
    ]);
    PoolConfig::resolve(&environment, DbRole::Api)
        .expect("the fixture connection string is well formed")
}

/// The same, for the queue the login surface and the fleet install reach.
fn unreachable_queue() -> RedisConfig {
    RedisConfig::from_url(RedisRole::Default, NOWHERE_QUEUE.to_owned())
        .with_request_timeout(std::time::Duration::from_millis(250))
}
