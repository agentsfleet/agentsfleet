//! A production router over seams a test drives, and no datastore anywhere.
//!
//! The exonum testkit pattern the milestone names: real HTTP through the real
//! router, over the real layers, with the things that reach a network replaced
//! at their declared seams. What that buys is that the refusal matrix is proven
//! against the thing that ships — including the layer ORDER, which a test
//! calling a handler directly cannot see at all.
//!
//! # Why the pool is real and unreachable rather than mocked
//!
//! There is no seam between `afd_fleet` and Postgres, deliberately: the
//! statements are the parity target, so a fake store would prove a handler
//! against SQL nobody runs. Instead the store holds a pool over an address that
//! answers nothing ([`afd_db::Db::unreachable`]), which is exactly what a
//! datastore outage looks like from the request path — and lets a suite prove
//! the transport-class refusal (RULE ECL) without stopping a container.
//!
//! A test that needs rows is an integration test, `#[ignore]`d and run by
//! `make test-integration-rustd`. This harness is for everything BEFORE the
//! first row is read, which is where §1's dimensions live.
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
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_fleet::Runners;
use afd_fleet::bundle::{Bundles, ContentHash};
use axum::Router;
use bytes::Bytes;
use object_store::ObjectStoreExt as _;
use object_store::memory::InMemory;

mod stubs_fleet;
mod stubs_runner;
mod stubs_tenant;
mod support;

pub(crate) use self::stubs_fleet::NoFleets;
pub(crate) use self::stubs_runner::NoWork;
pub(crate) use self::stubs_tenant::{
    DEPLOYMENT, NoBilling, NoDirectory, NoKeys, NoLogins, NoModels, NoTerminals, OWNED_WORKSPACE,
    OneWorkspace,
};
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

/// A fixed instant, so every row a verb writes is stamped predictably.
const FROZEN: i64 = 1_760_000_000_000;

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
    api_keys: NoKeys,
    now: UnixMillis,
}

impl Fleet {
    /// An instance whose dependencies answer, whose directory is empty, and
    /// whose Postgres is not there.
    pub(crate) fn new() -> Self {
        let directory = MockDirectory::new();
        let capabilities = MockCapabilities::new();
        let environment = MapEnv::from_pairs([(DbRole::Api.url_knob(), NOWHERE)]);
        let pool = PoolConfig::resolve(&environment, DbRole::Api)
            .expect("the fixture connection string is well formed");
        Self {
            ready: ReadyInputs {
                database: true,
                queue: true,
            },
            authenticator: Planes::new(directory.clone(), capabilities.clone(), NoVerifier),
            directory,
            capabilities,
            runners: Runners::new(Db::unreachable(&pool), Entropy::new()),
            leases: NoWork,
            // Unconfigured by default, so a suite that says nothing about
            // snapshots proves the refusal a deployment with no R2 knobs gives
            // — which is most of them.
            bundles: Bundles::unconfigured(),
            workspaces: OneWorkspace,
            api_keys: NoKeys,
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
    type Sessions = NoLogins;
    type Workspaces = OneWorkspace;
    type WorkspaceDirectory = NoDirectory;
    type ApiKeys = NoKeys;
    type CliCredentials = NoTerminals;
    type Fleets = NoFleets;
    type Billing = NoBilling;
    type Catalogue = NoModels;

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

    fn sessions(&self) -> &NoLogins {
        &NoLogins
    }

    fn workspaces(&self) -> &OneWorkspace {
        &self.workspaces
    }

    fn workspace_directory(&self) -> &NoDirectory {
        &NoDirectory
    }

    fn api_keys(&self) -> &NoKeys {
        &self.api_keys
    }

    fn cli_credentials(&self) -> &NoTerminals {
        &NoTerminals
    }

    fn fleets(&self) -> &NoFleets {
        &NoFleets
    }

    fn billing(&self) -> &NoBilling {
        &NoBilling
    }

    fn catalogue(&self) -> &NoModels {
        &NoModels
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
