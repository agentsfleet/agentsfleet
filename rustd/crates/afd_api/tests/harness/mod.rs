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
use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT, Planes, SchedulePlane, Services};
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

mod instance;
mod readiness;
mod stubs_ingress;
mod stubs_runner;
mod stubs_tenant;
mod support;

/// Signed deliveries, as a provider would present them.
pub(crate) mod webhook;

pub(crate) use self::instance::FIXTURE_APP_URL;
pub(crate) use self::stubs_ingress::{HarnessIngress, Recorded, Scripted};
pub(crate) use self::stubs_runner::NoWork;
pub(crate) use self::stubs_tenant::{DEPLOYMENT, OWNED_WORKSPACE, OneWorkspace};
pub(crate) use self::support::{
    file_runner, json_body, presented, runner_id, send, send_with_headers, tenant,
};

/// Where this fixture deployment's schedule fires would arrive.
///
/// A real destination shape, because it is half of what a fire token's subject
/// is checked against — a blank one would make every token fail the subject
/// check for a reason no test was about.
pub(crate) const SCHEDULE_DESTINATION: &str =
    "https://api.fixture.test/v1/ingress/qstash/schedules";

/// The seams a suite arranges, and the state the router is built over.
#[derive(Debug)]
pub(crate) struct Fleet {
    ready: ReadyInputs,
    mock_directory: MockDirectory,
    directory: Directory,
    capabilities: MockCapabilities,
    authenticator: Planes<Directory, MockCapabilities, NoVerifier>,
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
    ingress: HarnessIngress,
    schedules: SchedulePlane,
    connectors: afd_connector::Connectors,
    schedule_keys: Option<afd_cron::SigningKeys>,
    platform_admin: Option<Uuid7>,
    preferences: Preferences,
    approvals: Inbox,
    grants: IntegrationGrants,
    events: History,
    live: Live,
    analytics: Analytics,
    steering: Steer,
    memories: Memories,
    billing: Billing,
    catalogue: Models,
    runner_lease_history: RunnerLeaseHistory,
    admin_models: AdminModels,
    platform_keys: PlatformKeys,
    libraries: Libraries,
    library_imports: LibraryImports,
    now: UnixMillis,
}

/// The same auth seam backed either by the fast map or production Postgres.
///
/// `Fleet::new` files credentials into the map, which needs no datastore; the
/// live-router suites resolve the same seam through real Postgres so a scope
/// gate is proven against the rows a migration actually created.
#[derive(Debug, Clone)]
pub(crate) enum Directory {
    Mock(MockDirectory),
    Live(Credentials),
}

impl CredentialDirectory for Directory {
    async fn resolve(
        &self,
        kind: CredentialKind,
        digest: &Digest,
    ) -> Result<Option<CredentialRecord>, Unavailable> {
        match self {
            Self::Mock(directory) => directory.resolve(kind, digest).await,
            Self::Live(directory) => directory.resolve(kind, digest).await,
        }
    }
}

impl Fleet {
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

    /// The production router, over this instance.
    pub(crate) fn router(self) -> Router {
        let admission = Admission::new(DEFAULT_MAX_IN_FLIGHT);
        build(Arc::new(self), &admission)
    }
}

mod services;
